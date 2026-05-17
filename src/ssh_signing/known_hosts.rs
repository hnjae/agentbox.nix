// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use camino::Utf8Path;
use serde::Deserialize;
use tempfile::NamedTempFile;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

use crate::Result;
use crate::git::Git;
use crate::process::format_status;
use crate::runtime::RuntimeMount;

const CONTAINER_KNOWN_HOSTS: &str = "/run/agentbox/known_hosts";
const GIT_SSH_COMMAND_ENV: &str = "GIT_SSH_COMMAND";
const GIT_SSH_COMMAND: &str =
    "ssh -o UserKnownHostsFile=/run/agentbox/known_hosts -o StrictHostKeyChecking=yes";
const BACKUP_TIMESTAMP_FORMAT: &[FormatItem<'_>] =
    format_description!("[year][month][day]T[hour][minute][second]Z");

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AgentboxConfig {
    known_hosts: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgentboxConfig {
    #[serde(rename = "knownHosts")]
    known_hosts: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum KnownHostsLookupError {
    Unavailable(String),
    Failed(String),
}

type KnownHostsLookup = std::result::Result<Vec<String>, KnownHostsLookupError>;

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
        ssh_keygen_lookup,
        OffsetDateTime::now_utc(),
        warning,
    )
}

fn prepare_with<E, W>(
    git_root: &Utf8Path,
    environment: &mut E,
    mut remote_urls: impl FnMut(&Utf8Path) -> Result<Vec<String>>,
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

    let host_lines = host_known_hosts_lines(&remote_hosts, home, &mut lookup, warning);
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

fn load_config_with<E, W>(
    environment: &mut E,
    now: OffsetDateTime,
    warning: &mut W,
) -> AgentboxConfig
where
    E: FnMut(&str) -> Option<OsString> + ?Sized,
    W: FnMut(String) + ?Sized,
{
    let Some(path) = config_path(environment) else {
        warning(
            "cannot determine agentbox config path because neither XDG_CONFIG_HOME nor HOME is set; continuing with empty config".to_string(),
        );
        return AgentboxConfig::default();
    };

    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return AgentboxConfig::default();
        }
        Err(error) => {
            warning(format!(
                "failed to read agentbox config `{}`: {error}; continuing with empty config",
                path.display()
            ));
            return AgentboxConfig::default();
        }
    };

    match parse_config(&contents) {
        Ok(config) => config,
        Err(reason) => {
            let backup = backup_incompatible_config(&path, now);
            match backup {
                Ok(backup) => warning(format!(
                    "agentbox config `{}` is incompatible ({reason}); backed it up to `{}` and continuing with empty config",
                    path.display(),
                    backup.display()
                )),
                Err(error) => warning(format!(
                    "agentbox config `{}` is incompatible ({reason}); failed to back it up ({error}) and continuing with empty config",
                    path.display()
                )),
            }
            AgentboxConfig::default()
        }
    }
}

fn config_path<E>(environment: &mut E) -> Option<PathBuf>
where
    E: FnMut(&str) -> Option<OsString> + ?Sized,
{
    environment("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            environment("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .map(|config_home| config_home.join("agentbox/config.json"))
}

fn parse_config(contents: &str) -> std::result::Result<AgentboxConfig, String> {
    let raw: RawAgentboxConfig =
        serde_json::from_str(contents).map_err(|error| error.to_string())?;
    for entry in &raw.known_hosts {
        if entry.trim().is_empty() {
            return Err("knownHosts entries must not be blank".to_string());
        }
        if entry.contains('\n') || entry.contains('\r') {
            return Err("knownHosts entries must be single-line strings".to_string());
        }
    }
    Ok(AgentboxConfig {
        known_hosts: raw.known_hosts,
    })
}

fn backup_incompatible_config(path: &Path, now: OffsetDateTime) -> std::io::Result<PathBuf> {
    let timestamp = backup_timestamp(now);
    for suffix in std::iter::once(None).chain((1..).map(Some)) {
        let backup = backup_path(path, &timestamp, suffix);
        if backup.exists() {
            continue;
        }

        fs::rename(path, &backup)?;
        return Ok(backup);
    }

    unreachable!("unbounded suffix iterator must return a backup path")
}

fn backup_path(path: &Path, timestamp: &str, suffix: Option<usize>) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "config.json".into());
    let backup_name = match suffix {
        Some(suffix) => format!("{name}.bak.{timestamp}.{suffix}"),
        None => format!("{name}.bak.{timestamp}"),
    };
    path.with_file_name(backup_name)
}

fn backup_timestamp(now: OffsetDateTime) -> String {
    now.to_offset(UtcOffset::UTC)
        .format(BACKUP_TIMESTAMP_FORMAT)
        .unwrap_or_else(|_| "00000000T000000Z".to_string())
}

fn ssh_remote_hosts(urls: &[String]) -> Vec<String> {
    let mut hosts = Vec::new();
    let mut seen = HashSet::new();
    for url in urls {
        if let Some(host) = ssh_remote_host(url)
            && seen.insert(host.clone())
        {
            hosts.push(host);
        }
    }
    hosts
}

fn ssh_remote_host(url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("ssh://") {
        return ssh_url_host(rest);
    }
    if let Some(rest) = url.strip_prefix("git+ssh://") {
        return ssh_url_host(rest);
    }

    scp_like_host(url)
}

fn ssh_url_host(rest: &str) -> Option<String> {
    let authority = rest.split('/').next()?;
    if authority.is_empty() {
        return None;
    }

    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.is_empty() {
        return None;
    }

    if let Some(after_bracket) = authority.strip_prefix('[') {
        let (host, remainder) = after_bracket.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        let port = remainder.strip_prefix(':').and_then(parse_port);
        return Some(format_known_host(host, port));
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, parse_port(port)),
        _ => (authority, None),
    };
    if host.is_empty() {
        None
    } else {
        Some(format_known_host(host, port))
    }
}

fn scp_like_host(url: &str) -> Option<String> {
    if url.contains("://") {
        return None;
    }
    let (host, _path) = url.split_once(':')?;
    if host.is_empty() || host.contains('/') {
        return None;
    }

    let host = host.rsplit('@').next().unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn parse_port(port: &str) -> Option<u16> {
    port.parse::<u16>().ok()
}

fn format_known_host(host: &str, port: Option<u16>) -> String {
    match port {
        Some(22) | None => host.to_string(),
        Some(port) => format!("[{host}]:{port}"),
    }
}

fn host_known_hosts_lines<W>(
    remote_hosts: &[String],
    home: Option<OsString>,
    lookup: &mut impl FnMut(&str, &Path) -> KnownHostsLookup,
    warning: &mut W,
) -> Vec<String>
where
    W: FnMut(String) + ?Sized,
{
    if remote_hosts.is_empty() {
        return Vec::new();
    }

    let files = known_hosts_files(home);
    let mut lines = Vec::new();
    for host in remote_hosts {
        let mut host_lines = Vec::new();
        for file in &files {
            match lookup(host, file) {
                Ok(matches) => host_lines.extend(matches),
                Err(KnownHostsLookupError::Unavailable(reason)) => {
                    warning(format!(
                        "ssh-keygen is unavailable for SSH known_hosts lookup ({reason}); continuing with config-provided knownHosts only"
                    ));
                    return Vec::new();
                }
                Err(KnownHostsLookupError::Failed(reason)) => {
                    warning(format!(
                        "ssh-keygen lookup failed for SSH remote host `{host}` ({reason}); continuing with config-provided knownHosts only"
                    ));
                    return Vec::new();
                }
            }
        }

        if host_lines.is_empty() {
            warning(format!(
                "no known_hosts entry found for SSH remote host `{host}`; Git SSH host verification may fail"
            ));
        }
        lines.extend(host_lines);
    }

    lines
}

fn known_hosts_files(home: Option<OsString>) -> Vec<PathBuf> {
    let Some(home) = home.filter(|home| !home.is_empty()) else {
        return Vec::new();
    };
    let ssh_dir = PathBuf::from(home).join(".ssh");
    ["known_hosts", "known_hosts2"]
        .into_iter()
        .map(|name| ssh_dir.join(name))
        .filter(|path| path.is_file())
        .collect()
}

fn ssh_keygen_lookup(host: &str, file: &Path) -> KnownHostsLookup {
    let output = Command::new("ssh-keygen")
        .arg("-F")
        .arg(host)
        .arg("-f")
        .arg(file)
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(KnownHostsLookupError::Unavailable(error.to_string()));
        }
        Err(error) => return Err(KnownHostsLookupError::Failed(error.to_string())),
    };

    match output.status.code() {
        Some(0) => Ok(parse_ssh_keygen_lines(&String::from_utf8_lossy(
            &output.stdout,
        ))),
        Some(1) => Ok(Vec::new()),
        _ => Err(KnownHostsLookupError::Failed(format!(
            "{}: {}",
            format_status(output.status),
            output_detail(&output.stdout, &output.stderr)
        ))),
    }
}

fn parse_ssh_keygen_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(ToOwned::to_owned)
        .collect()
}

fn output_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = String::from_utf8_lossy(stdout);
    [stderr.trim(), stdout.trim()]
        .into_iter()
        .find(|detail| !detail.is_empty())
        .unwrap_or("no output")
        .to_string()
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

    use super::*;

    #[test]
    fn missing_config_returns_empty_config() {
        let sandbox = tempfile::tempdir().unwrap();
        let mut warnings = Vec::new();

        let config = load_config_with(
            &mut |name| test_config_env(name, sandbox.path()),
            sample_time(),
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(config, AgentboxConfig::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn valid_known_hosts_config_loads() {
        let sandbox = tempfile::tempdir().unwrap();
        let config_path = sandbox.path().join("agentbox/config.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            r#"{"knownHosts":["github.com ssh-ed25519 AAAA","[git.example.com]:2222 ssh-ed25519 BBBB"]}"#,
        )
        .unwrap();

        let config = load_config_with(
            &mut |name| test_config_env(name, sandbox.path()),
            sample_time(),
            &mut panic_warning,
        );

        assert_eq!(
            config.known_hosts,
            [
                "github.com ssh-ed25519 AAAA",
                "[git.example.com]:2222 ssh-ed25519 BBBB"
            ]
        );
    }

    #[test]
    fn invalid_config_is_backed_up_and_ignored() {
        let sandbox = tempfile::tempdir().unwrap();
        let config_path = sandbox.path().join("agentbox/config.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, "{not json").unwrap();
        let mut warnings = Vec::new();

        let config = load_config_with(
            &mut |name| test_config_env(name, sandbox.path()),
            sample_time(),
            &mut |warning| warnings.push(warning),
        );

        let backup = sandbox
            .path()
            .join("agentbox/config.json.bak.20260517T112345Z");
        assert_eq!(config, AgentboxConfig::default());
        assert!(!config_path.exists());
        assert_eq!(fs::read_to_string(backup).unwrap(), "{not json");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("incompatible"));
    }

    #[test]
    fn invalid_config_backup_collision_appends_numeric_suffix() {
        let sandbox = tempfile::tempdir().unwrap();
        let config_path = sandbox.path().join("agentbox/config.json");
        let first_backup = sandbox
            .path()
            .join("agentbox/config.json.bak.20260517T112345Z");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, r#"{"knownHosts":[""]}"#).unwrap();
        fs::write(&first_backup, "previous").unwrap();

        let config = load_config_with(
            &mut |name| test_config_env(name, sandbox.path()),
            sample_time(),
            &mut |_| {},
        );

        let second_backup = sandbox
            .path()
            .join("agentbox/config.json.bak.20260517T112345Z.1");
        assert_eq!(config, AgentboxConfig::default());
        assert_eq!(fs::read_to_string(first_backup).unwrap(), "previous");
        assert_eq!(
            fs::read_to_string(second_backup).unwrap(),
            r#"{"knownHosts":[""]}"#
        );
    }

    #[test]
    fn config_rejects_unknown_fields_and_multiline_entries() {
        assert!(parse_config(r#"{"knownHosts":[],"extra":true}"#).is_err());
        assert!(parse_config("{\"knownHosts\":[\"github.com ssh-ed25519 AAAA\\nnext\"]}").is_err());
    }

    #[test]
    fn parses_ssh_remote_hosts() {
        let urls = vec![
            "git@github.com:owner/repo.git".to_string(),
            "ssh://git@gitlab.com/group/repo.git".to_string(),
            "ssh://git@example.com:2222/repo.git".to_string(),
            "git+ssh://git@example.net/repo.git".to_string(),
            "https://github.com/owner/repo.git".to_string(),
            "/home/alice/repo".to_string(),
            "../repo".to_string(),
        ];

        assert_eq!(
            ssh_remote_hosts(&urls),
            [
                "github.com",
                "gitlab.com",
                "[example.com]:2222",
                "example.net"
            ]
        );
    }

    #[test]
    fn parses_ssh_keygen_output_lines_without_comments() {
        assert_eq!(
            parse_ssh_keygen_lines(
                "# Host github.com found: line 1\ngithub.com ssh-ed25519 AAAA\n\n# another\n|1|hash ssh-rsa BBBB\n"
            ),
            ["github.com ssh-ed25519 AAAA", "|1|hash ssh-rsa BBBB"]
        );
    }

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

    fn test_config_env(name: &str, config_home: &Path) -> Option<OsString> {
        match name {
            "XDG_CONFIG_HOME" => Some(config_home.as_os_str().to_os_string()),
            _ => None,
        }
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
