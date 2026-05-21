// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::process::{ProcessCaptureError, ProcessRunner, ProcessStatusOutput, format_status};

use super::remote::{SshRemoteHost, format_known_host, parse_port};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum KnownHostsLookupError {
    Unavailable(String),
    Failed(String),
}

pub(super) type KnownHostsLookup = std::result::Result<Vec<String>, KnownHostsLookupError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SshHostConfig {
    pub(super) lookup_hosts: Vec<String>,
    pub(super) known_hosts_files: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SshConfigLookupError {
    Unavailable(String),
    Failed(String),
}

pub(super) type SshConfigLookup = std::result::Result<SshHostConfig, SshConfigLookupError>;

#[derive(Debug, Clone, Default)]
pub(super) struct SshLookupCommands {
    runner: ProcessRunner,
}

impl SshLookupCommands {
    pub(super) fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn with_runner(runner: ProcessRunner) -> Self {
        Self { runner }
    }

    pub(super) fn ssh_config_lookup(&self, host: &SshRemoteHost) -> SshConfigLookup {
        let output = self.capture_status("ssh", |command| {
            command.arg("-G");
            if let Some(port) = host.port {
                command.arg("-p").arg(port.to_string());
            }
            command.arg("--").arg(&host.config_host);
        })?;

        if output.status.success() {
            return Ok(parse_ssh_config_output(&output.stdout, host));
        }

        Err(SshConfigLookupError::Failed(format!(
            "{}: {}",
            format_status(output.status),
            output.output_detail()
        )))
    }

    pub(super) fn ssh_keygen_lookup(&self, host: &str, file: &Path) -> KnownHostsLookup {
        let output = self.capture_status("ssh-keygen", |command| {
            command.arg("-F").arg(host).arg("-f").arg(file);
        })?;

        match output.status.code() {
            Some(0) => Ok(parse_ssh_keygen_lines(&output.stdout)),
            Some(1) => Ok(Vec::new()),
            _ => Err(KnownHostsLookupError::Failed(format!(
                "{}: {}",
                format_status(output.status),
                output.output_detail()
            ))),
        }
    }

    fn capture_status(
        &self,
        program: &str,
        configure: impl FnOnce(&mut std::process::Command),
    ) -> std::result::Result<ProcessStatusOutput, CommandLookupError> {
        self.runner
            .try_capture_status(program, configure)
            .map_err(CommandLookupError::from)
    }
}

enum CommandLookupError {
    Unavailable(String),
    Failed(String),
}

impl From<ProcessCaptureError> for CommandLookupError {
    fn from(error: ProcessCaptureError) -> Self {
        let unavailable = error.is_not_found();
        let message = error.into_error().to_string();
        if unavailable {
            Self::Unavailable(message)
        } else {
            Self::Failed(message)
        }
    }
}

impl From<CommandLookupError> for SshConfigLookupError {
    fn from(error: CommandLookupError) -> Self {
        match error {
            CommandLookupError::Unavailable(message) => Self::Unavailable(message),
            CommandLookupError::Failed(message) => Self::Failed(message),
        }
    }
}

impl From<CommandLookupError> for KnownHostsLookupError {
    fn from(error: CommandLookupError) -> Self {
        match error {
            CommandLookupError::Unavailable(message) => Self::Unavailable(message),
            CommandLookupError::Failed(message) => Self::Failed(message),
        }
    }
}

pub(super) fn host_known_hosts_lines<W>(
    remote_hosts: &[SshRemoteHost],
    home: Option<OsString>,
    ssh_config: &mut impl FnMut(&SshRemoteHost) -> SshConfigLookup,
    lookup: &mut impl FnMut(&str, &Path) -> KnownHostsLookup,
    warning: &mut W,
) -> Vec<String>
where
    W: FnMut(String) + ?Sized,
{
    if remote_hosts.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for host in remote_hosts {
        let config = match ssh_config(host) {
            Ok(config) => config,
            Err(SshConfigLookupError::Unavailable(reason)) => {
                warning(format!(
                    "ssh is unavailable for SSH config lookup for remote host `{}` ({reason}); falling back to $HOME/.ssh/known_hosts and $HOME/.ssh/known_hosts2",
                    host.known_hosts_name()
                ));
                fallback_ssh_host_config(host, home.clone())
            }
            Err(SshConfigLookupError::Failed(reason)) => {
                warning(format!(
                    "ssh -G lookup failed for SSH remote host `{}` ({reason}); falling back to $HOME/.ssh/known_hosts and $HOME/.ssh/known_hosts2",
                    host.known_hosts_name()
                ));
                fallback_ssh_host_config(host, home.clone())
            }
        };
        let mut host_lines = Vec::new();
        for file in &config.known_hosts_files {
            for lookup_host in &config.lookup_hosts {
                match lookup(lookup_host, file) {
                    Ok(matches) => host_lines.extend(matches),
                    Err(KnownHostsLookupError::Unavailable(reason)) => {
                        warning(format!(
                            "ssh-keygen is unavailable for SSH known_hosts lookup ({reason}); continuing with config-provided knownHosts only"
                        ));
                        return Vec::new();
                    }
                    Err(KnownHostsLookupError::Failed(reason)) => {
                        warning(format!(
                            "ssh-keygen lookup failed for SSH remote host `{}` ({reason}); continuing with config-provided knownHosts only",
                            host.known_hosts_name()
                        ));
                        return Vec::new();
                    }
                }
            }
        }

        if host_lines.is_empty() {
            warning(format!(
                "no known_hosts entry found for SSH remote host `{}`; Git SSH host verification may fail",
                host.known_hosts_name()
            ));
        }
        lines.extend(host_lines);
    }

    lines
}

fn fallback_ssh_host_config(host: &SshRemoteHost, home: Option<OsString>) -> SshHostConfig {
    SshHostConfig {
        lookup_hosts: vec![host.known_hosts_name()],
        known_hosts_files: known_hosts_files(home),
    }
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

pub(super) fn ssh_config_lookup(host: &SshRemoteHost) -> SshConfigLookup {
    SshLookupCommands::new().ssh_config_lookup(host)
}

fn parse_ssh_config_output(output: &str, host: &SshRemoteHost) -> SshHostConfig {
    let mut hostname = None;
    let mut port = host.port;
    let mut host_key_alias = None;
    let mut known_hosts_files = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some((key, values)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let values = values.trim();
        match key.to_ascii_lowercase().as_str() {
            "hostname" => hostname = first_ssh_config_value(values).map(ToOwned::to_owned),
            "port" => port = first_ssh_config_value(values).and_then(parse_port),
            "hostkeyalias" => {
                host_key_alias = first_ssh_config_value(values).map(ToOwned::to_owned);
            }
            "userknownhostsfile" | "globalknownhostsfile" => {
                for value in values.split_whitespace() {
                    push_known_hosts_file(&mut known_hosts_files, value);
                }
            }
            _ => {}
        }
    }

    SshHostConfig {
        lookup_hosts: ssh_config_lookup_hosts(
            host,
            hostname.as_deref(),
            port,
            host_key_alias.as_deref(),
        ),
        known_hosts_files,
    }
}

fn first_ssh_config_value(values: &str) -> Option<&str> {
    values
        .split_whitespace()
        .find(|value| !value.is_empty() && *value != "none")
}

fn push_known_hosts_file(files: &mut Vec<PathBuf>, value: &str) {
    if value == "none" {
        return;
    }

    let path = PathBuf::from(value);
    if path.is_file() && !files.contains(&path) {
        files.push(path);
    }
}

fn ssh_config_lookup_hosts(
    host: &SshRemoteHost,
    hostname: Option<&str>,
    port: Option<u16>,
    host_key_alias: Option<&str>,
) -> Vec<String> {
    let mut hosts = Vec::new();
    push_unique_host(&mut hosts, host_key_alias.map(ToOwned::to_owned));
    let resolved_host = hostname.unwrap_or(&host.config_host);
    push_unique_host(&mut hosts, Some(format_known_host(resolved_host, port)));
    push_unique_host(&mut hosts, Some(host.known_hosts_name()));
    hosts
}

fn push_unique_host(hosts: &mut Vec<String>, host: Option<String>) {
    let Some(host) = host.filter(|host| !host.is_empty()) else {
        return;
    };
    if !hosts.contains(&host) {
        hosts.push(host);
    }
}

pub(super) fn ssh_keygen_lookup(host: &str, file: &Path) -> KnownHostsLookup {
    SshLookupCommands::new().ssh_keygen_lookup(host, file)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::process::ProcessRunner;

    #[test]
    fn parses_ssh_config_lookup_hosts_and_files() {
        let sandbox = tempfile::tempdir().unwrap();
        let user_file = sandbox.path().join("known_hosts.custom");
        let global_file = sandbox.path().join("ssh_known_hosts");
        fs::write(&user_file, "").unwrap();
        fs::write(&global_file, "").unwrap();
        let remote_host = SshRemoteHost {
            config_host: "github-work".to_string(),
            port: None,
        };
        let output = format!(
            "host github-work\nhostname ssh.github.com\nport 443\nhostkeyalias github.com\nuserknownhostsfile {} missing\nuserknownhostsfile none\nglobalknownhostsfile {}\n",
            user_file.display(),
            global_file.display()
        );

        let config = parse_ssh_config_output(&output, &remote_host);

        assert_eq!(
            config.lookup_hosts,
            ["github.com", "[ssh.github.com]:443", "github-work"]
        );
        assert_eq!(config.known_hosts_files, [user_file, global_file]);
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

    #[cfg(unix)]
    #[test]
    fn ssh_config_lookup_uses_injected_process_runner() {
        let sandbox = tempfile::tempdir().unwrap();
        let known_hosts = sandbox.path().join("known_hosts.custom");
        fs::write(&known_hosts, "").unwrap();
        write_executable(
            sandbox.path(),
            "ssh",
            &format!(
                "#!/bin/sh\n\
                 if [ \"$1\" != \"-G\" ] || [ \"$2\" != \"-p\" ] || [ \"$3\" != \"443\" ] || [ \"$4\" != \"--\" ] || [ \"$5\" != \"github-work\" ]; then\n\
                 \tprintf 'unexpected args: %s\\n' \"$*\" >&2\n\
                 \texit 42\n\
                 fi\n\
                 printf 'hostname ssh.github.com\\nport 443\\nhostkeyalias github.com\\nuserknownhostsfile {}\\n'\n",
                known_hosts.display()
            ),
        );
        let commands =
            SshLookupCommands::with_runner(ProcessRunner::new().with_path_prepend(sandbox.path()));
        let remote_host = SshRemoteHost {
            config_host: "github-work".to_string(),
            port: Some(443),
        };

        let config = commands.ssh_config_lookup(&remote_host).unwrap();

        assert_eq!(
            config.lookup_hosts,
            ["github.com", "[ssh.github.com]:443", "[github-work]:443"]
        );
        assert_eq!(config.known_hosts_files, [known_hosts]);
    }

    #[cfg(unix)]
    #[test]
    fn ssh_keygen_lookup_uses_injected_process_runner() {
        let sandbox = tempfile::tempdir().unwrap();
        let known_hosts = sandbox.path().join("known_hosts");
        fs::write(&known_hosts, "github.com ssh-ed25519 AAAA\n").unwrap();
        write_executable(
            sandbox.path(),
            "ssh-keygen",
            "#!/bin/sh\n\
             if [ \"$1\" != \"-F\" ] || [ \"$2\" != \"github.com\" ] || [ \"$3\" != \"-f\" ]; then\n\
             \tprintf 'unexpected args: %s\\n' \"$*\" >&2\n\
             \texit 42\n\
             fi\n\
             printf '# Host github.com found: line 1\\ngithub.com ssh-ed25519 AAAA\\n'\n",
        );
        let commands =
            SshLookupCommands::with_runner(ProcessRunner::new().with_path_prepend(sandbox.path()));

        let lines = commands
            .ssh_keygen_lookup("github.com", &known_hosts)
            .unwrap();

        assert_eq!(lines, ["github.com ssh-ed25519 AAAA"]);
    }

    #[cfg(unix)]
    fn write_executable(directory: &Path, name: &str, contents: &str) {
        use std::os::unix::fs::PermissionsExt;

        let path = directory.join(name);
        fs::write(&path, contents).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}
