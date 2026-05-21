// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use camino::Utf8Path;
use tempfile::NamedTempFile;
use time::OffsetDateTime;

use crate::Result;
use crate::git::Git;
use crate::runtime::RuntimeMount;

mod config;
mod lookup;
mod remote;

use config::load_config_with;
use lookup::{
    KnownHostsLookup, SshConfigLookup, host_known_hosts_lines, ssh_config_lookup, ssh_keygen_lookup,
};
use remote::{SshRemoteHost, ssh_remote_hosts};

const CONTAINER_KNOWN_HOSTS: &str = "/run/agentbox/known_hosts";
const GIT_SSH_COMMAND_ENV: &str = "GIT_SSH_COMMAND";
const GIT_SSH_COMMAND: &str =
    "ssh -o UserKnownHostsFile=/run/agentbox/known_hosts -o StrictHostKeyChecking=yes";

#[derive(Debug)]
pub(super) struct PreparedKnownHosts {
    mount: RuntimeMount,
    env: BTreeMap<String, String>,
    file: NamedTempFile,
}

impl PreparedKnownHosts {
    pub(super) fn into_parts(self) -> (RuntimeMount, BTreeMap<String, String>, NamedTempFile) {
        (self.mount, self.env, self.file)
    }
}

pub(super) fn prepare<E, W>(
    git_root: &Utf8Path,
    environment: &mut E,
    warning: &mut W,
) -> Option<PreparedKnownHosts>
where
    E: FnMut(&str) -> Option<OsString> + ?Sized,
    W: FnMut(String) + ?Sized,
{
    let git = Git::new();
    prepare_with(
        git_root,
        environment,
        |git_root| git.remote_urls(git_root),
        ssh_config_lookup,
        ssh_keygen_lookup,
        OffsetDateTime::now_utc(),
        warning,
    )
}

fn prepare_with<E, W>(
    git_root: &Utf8Path,
    environment: &mut E,
    mut remote_urls: impl FnMut(&Utf8Path) -> Result<Vec<String>>,
    mut ssh_config: impl FnMut(&SshRemoteHost) -> SshConfigLookup,
    mut lookup: impl FnMut(&str, &Path) -> KnownHostsLookup,
    now: OffsetDateTime,
    warning: &mut W,
) -> Option<PreparedKnownHosts>
where
    E: FnMut(&str) -> Option<OsString> + ?Sized,
    W: FnMut(String) + ?Sized,
{
    let config = load_config_with(environment, now, warning);
    let remote_hosts = match remote_urls(git_root) {
        Ok(urls) => ssh_remote_hosts(&urls),
        Err(error) => {
            warning(format!(
                "failed to inspect Git remotes for SSH known_hosts passthrough: {error}; continuing without host known_hosts entries"
            ));
            Vec::new()
        }
    };
    let home = environment("HOME");

    let host_lines =
        host_known_hosts_lines(&remote_hosts, home, &mut ssh_config, &mut lookup, warning);
    let lines = combined_known_hosts_lines(host_lines, config.known_hosts);
    if lines.is_empty() {
        return None;
    }

    match write_temporary_known_hosts(&lines) {
        Ok(file) => {
            let source = file.path().to_string_lossy().into_owned();
            Some(PreparedKnownHosts {
                mount: RuntimeMount::read_only_bind(source, CONTAINER_KNOWN_HOSTS),
                env: BTreeMap::from([(
                    GIT_SSH_COMMAND_ENV.to_string(),
                    GIT_SSH_COMMAND.to_string(),
                )]),
                file,
            })
        }
        Err(error) => {
            warning(format!(
                "failed to prepare temporary SSH known_hosts file: {error}; Git SSH host verification passthrough disabled"
            ));
            None
        }
    }
}

fn combined_known_hosts_lines(host_lines: Vec<String>, config_lines: Vec<String>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut seen = HashSet::new();
    for line in host_lines.into_iter().chain(config_lines) {
        if seen.insert(line.clone()) {
            lines.push(line);
        }
    }
    lines
}

fn write_temporary_known_hosts(lines: &[String]) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    for line in lines {
        writeln!(file, "{line}")?;
    }
    file.flush()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::lookup::{KnownHostsLookupError, SshConfigLookupError, SshHostConfig};
    use super::*;

    #[test]
    fn unavailable_ssh_keygen_uses_config_entries_only() {
        let sandbox = tempfile::tempdir().unwrap();
        let config_path = sandbox.path().join("agentbox/config.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            r#"{"knownHosts":["extra.example ssh-ed25519 CCCC"]}"#,
        )
        .unwrap();
        let home = tempfile::tempdir().unwrap();
        let ssh_dir = home.path().join(".ssh");
        fs::create_dir(&ssh_dir).unwrap();
        fs::write(ssh_dir.join("known_hosts"), "placeholder").unwrap();
        let mut warnings = Vec::new();

        let prepared = prepare_with(
            Utf8Path::new("/repo"),
            &mut |name| match name {
                "XDG_CONFIG_HOME" => Some(sandbox.path().as_os_str().to_os_string()),
                "HOME" => Some(home.path().as_os_str().to_os_string()),
                _ => None,
            },
            |_git_root| Ok(vec!["git@github.com:owner/repo.git".to_string()]),
            |_host| {
                Ok(SshHostConfig {
                    lookup_hosts: vec!["github.com".to_string()],
                    known_hosts_files: vec![ssh_dir.join("known_hosts")],
                })
            },
            |_host, _file| Err(KnownHostsLookupError::Unavailable("missing".to_string())),
            sample_time(),
            &mut |warning| warnings.push(warning),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(prepared.file.path()).unwrap(),
            "extra.example ssh-ed25519 CCCC\n"
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("ssh-keygen is unavailable"));
    }

    #[test]
    fn unavailable_ssh_config_falls_back_to_home_known_hosts_files() {
        let sandbox = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let ssh_dir = home.path().join(".ssh");
        fs::create_dir(&ssh_dir).unwrap();
        let known_hosts = ssh_dir.join("known_hosts");
        fs::write(&known_hosts, "placeholder").unwrap();
        let mut warnings = Vec::new();

        let prepared = prepare_with(
            Utf8Path::new("/repo"),
            &mut |name| match name {
                "XDG_CONFIG_HOME" => Some(sandbox.path().as_os_str().to_os_string()),
                "HOME" => Some(home.path().as_os_str().to_os_string()),
                _ => None,
            },
            |_git_root| Ok(vec!["git@github.com:owner/repo.git".to_string()]),
            |_host| Err(SshConfigLookupError::Unavailable("missing".to_string())),
            |host, file| {
                assert_eq!(host, "github.com");
                assert_eq!(file, known_hosts.as_path());
                Ok(vec!["github.com ssh-ed25519 AAAAHOST".to_string()])
            },
            sample_time(),
            &mut |warning| warnings.push(warning),
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(prepared.file.path()).unwrap(),
            "github.com ssh-ed25519 AAAAHOST\n"
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("ssh is unavailable"));
    }

    #[test]
    fn prepare_combines_host_and_config_known_hosts_lines() {
        let sandbox = tempfile::tempdir().unwrap();
        let config_path = sandbox.path().join("agentbox/config.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            r#"{"knownHosts":["github.com ssh-ed25519 AAAA","extra.example ssh-ed25519 CCCC"]}"#,
        )
        .unwrap();
        let home = tempfile::tempdir().unwrap();
        let ssh_dir = home.path().join(".ssh");
        fs::create_dir(&ssh_dir).unwrap();
        let known_hosts = ssh_dir.join("known_hosts");
        fs::write(&known_hosts, "placeholder").unwrap();

        let prepared = prepare_with(
            Utf8Path::new("/repo"),
            &mut |name| match name {
                "XDG_CONFIG_HOME" => Some(sandbox.path().as_os_str().to_os_string()),
                "HOME" => Some(home.path().as_os_str().to_os_string()),
                _ => None,
            },
            |_git_root| Ok(vec!["git@github.com:owner/repo.git".to_string()]),
            |_host| {
                Ok(SshHostConfig {
                    lookup_hosts: vec!["github.com".to_string()],
                    known_hosts_files: vec![known_hosts.clone()],
                })
            },
            |host, file| {
                assert_eq!(host, "github.com");
                assert_eq!(file, known_hosts.as_path());
                Ok(vec![
                    "github.com ssh-ed25519 AAAA".to_string(),
                    "|1|hashed ssh-ed25519 BBBB".to_string(),
                ])
            },
            sample_time(),
            &mut panic_warning,
        )
        .unwrap();

        let contents = fs::read_to_string(prepared.file.path()).unwrap();
        assert_eq!(
            contents,
            "github.com ssh-ed25519 AAAA\n|1|hashed ssh-ed25519 BBBB\nextra.example ssh-ed25519 CCCC\n"
        );
        assert!(prepared.mount.read_only);
        assert_eq!(prepared.mount.destination, CONTAINER_KNOWN_HOSTS);
        assert_eq!(
            prepared.env.get(GIT_SSH_COMMAND_ENV).map(String::as_str),
            Some(GIT_SSH_COMMAND)
        );
    }

    fn sample_time() -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::May, 17).unwrap(),
            Time::from_hms(11, 23, 45).unwrap(),
        )
        .assume_utc()
    }

    fn panic_warning(warning: String) {
        panic!("unexpected warning: {warning}");
    }
}
