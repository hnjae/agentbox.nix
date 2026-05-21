// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use crate::process::{ProcessCaptureError, ProcessRunner, ProcessStatusOutput};
use crate::ssh_signing::known_hosts::remote::SshRemoteHost;

use super::config::{SshConfigLookup, SshConfigLookupError, parse_ssh_config_output};
use super::keygen::{KnownHostsLookup, KnownHostsLookupError, parse_ssh_keygen_lines};

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

        Err(SshConfigLookupError::Failed(
            output.status_with_output_detail(),
        ))
    }

    pub(super) fn ssh_keygen_lookup(&self, host: &str, file: &Path) -> KnownHostsLookup {
        let output = self.capture_status("ssh-keygen", |command| {
            command.arg("-F").arg(host).arg("-f").arg(file);
        })?;

        match output.status.code() {
            Some(0) => Ok(parse_ssh_keygen_lines(&output.stdout)),
            Some(1) => Ok(Vec::new()),
            _ => Err(KnownHostsLookupError::Failed(
                output.status_with_output_detail(),
            )),
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

pub(in crate::ssh_signing::known_hosts) fn ssh_config_lookup(
    host: &SshRemoteHost,
) -> SshConfigLookup {
    SshLookupCommands::new().ssh_config_lookup(host)
}

pub(in crate::ssh_signing::known_hosts) fn ssh_keygen_lookup(
    host: &str,
    file: &Path,
) -> KnownHostsLookup {
    SshLookupCommands::new().ssh_keygen_lookup(host, file)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::process::ProcessRunner;

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
