// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use camino::Utf8Path;

use crate::Result;
use crate::diagnostic;
use crate::git::Git;
use crate::runtime::{RuntimeMount, RuntimeRunSpec};

pub(crate) const CONTAINER_SSH_AUTH_SOCK: &str = "/run/agentbox/ssh-agent.sock";

mod agent_socket;
mod git_config;
mod known_hosts;
mod signing_key;

use agent_socket::{HOST_SSH_AUTH_SOCK_ENV, detect_host_agent_socket, utf8_path};
use git_config::{append_git_config_env, read_git_config_entries};
use known_hosts::PreparedKnownHosts;

#[derive(Debug, Default)]
pub(crate) struct SshPassthroughGuard {
    _known_hosts_file: Option<tempfile::NamedTempFile>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SshCommitSigningPassthrough {
    agent_socket_mount: Option<RuntimeMount>,
    env: BTreeMap<String, String>,
}

impl SshCommitSigningPassthrough {
    fn apply_to(self, run_spec: &mut RuntimeRunSpec) {
        let create = run_spec.create_mut();
        if let Some(mount) = self.agent_socket_mount {
            create.mounts.push(mount);
        }
        create.default_env.extend(self.env);
    }
}

#[derive(Debug, Default)]
struct SshKnownHostsPassthrough {
    prepared: Option<PreparedKnownHosts>,
}

impl SshKnownHostsPassthrough {
    fn apply_to(self, run_spec: &mut RuntimeRunSpec) -> SshPassthroughGuard {
        let Some(prepared) = self.prepared else {
            return SshPassthroughGuard::default();
        };
        let (mount, env, file) = prepared.into_parts();
        let create = run_spec.create_mut();
        create.mounts.push(mount);
        create.default_env.extend(env);
        SshPassthroughGuard {
            _known_hosts_file: Some(file),
        }
    }
}

#[derive(Debug, Default)]
struct SshPassthrough {
    commit_signing: SshCommitSigningPassthrough,
    known_hosts: SshKnownHostsPassthrough,
}

impl SshPassthrough {
    fn apply_to(self, run_spec: &mut RuntimeRunSpec) -> SshPassthroughGuard {
        self.commit_signing.apply_to(run_spec);
        self.known_hosts.apply_to(run_spec)
    }
}

pub(crate) fn apply_ssh_passthrough(
    run_spec: &mut RuntimeRunSpec,
    git_root: &Utf8Path,
) -> SshPassthroughGuard {
    let git = Git::new();
    detect_with(
        git_root,
        |name| std::env::var_os(name),
        |git_root, key| git.config_get(git_root, key),
        |git_root, environment, warning| known_hosts::prepare(git_root, environment, warning),
        diagnostic::warning,
    )
    .apply_to(run_spec)
}

fn detect_with(
    git_root: &Utf8Path,
    mut environment: impl FnMut(&str) -> Option<std::ffi::OsString>,
    mut git_config: impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    mut known_hosts: impl FnMut(
        &Utf8Path,
        &mut dyn FnMut(&str) -> Option<std::ffi::OsString>,
        &mut dyn FnMut(String),
    ) -> Option<PreparedKnownHosts>,
    mut warning: impl FnMut(String),
) -> SshPassthrough {
    let Some(host_socket) = detect_host_agent_socket(&mut environment, &mut warning) else {
        return SshPassthrough::default();
    };

    let home = environment("HOME").and_then(utf8_path);
    let mut env = BTreeMap::from([(
        HOST_SSH_AUTH_SOCK_ENV.to_string(),
        CONTAINER_SSH_AUTH_SOCK.to_string(),
    )]);
    let git_entries =
        read_git_config_entries(git_root, home.as_deref(), &mut git_config, &mut warning);
    append_git_config_env(&mut env, &git_entries);
    let known_hosts = known_hosts(git_root, &mut environment, &mut warning);

    SshPassthrough {
        commit_signing: SshCommitSigningPassthrough {
            agent_socket_mount: Some(RuntimeMount::bind(
                host_socket.to_string(),
                CONTAINER_SSH_AUTH_SOCK,
            )),
            env,
        },
        known_hosts: SshKnownHostsPassthrough {
            prepared: known_hosts,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use camino::Utf8PathBuf;

    use super::git_config::{GIT_CONFIG_COUNT_ENV, GIT_CONFIG_KEYS};
    use super::*;
    use crate::runtime::{RuntimeCreateSpec, RuntimeRunSpec};

    #[test]
    fn unset_ssh_auth_sock_adds_no_mount_env_or_git_config() {
        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            |_| None,
            |_git_root, _key| panic!("git config must not be read without an agent socket"),
            |_git_root, _environment, _warning| {
                panic!("known_hosts must not be read without an agent socket")
            },
            panic_warning,
        );

        assert!(passthrough.commit_signing.agent_socket_mount.is_none());
        assert!(passthrough.commit_signing.env.is_empty());
        assert!(passthrough.known_hosts.prepared.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn valid_ssh_auth_sock_adds_socket_mount_and_container_env() {
        let (_sandbox, socket_path, _listener) = bind_test_socket();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            |name| test_env(name, &socket_path, None),
            |_git_root, _key| Ok(None),
            |_git_root, _environment, _warning| None,
            panic_warning,
        );

        assert_eq!(
            passthrough.commit_signing.agent_socket_mount,
            Some(RuntimeMount::bind(
                socket_path.to_string(),
                CONTAINER_SSH_AUTH_SOCK
            ))
        );
        assert_eq!(
            passthrough
                .commit_signing
                .env
                .get(HOST_SSH_AUTH_SOCK_ENV)
                .map(String::as_str),
            Some(CONTAINER_SSH_AUTH_SOCK)
        );
        assert!(
            !passthrough
                .commit_signing
                .env
                .contains_key(GIT_CONFIG_COUNT_ENV)
        );
    }

    #[test]
    fn invalid_ssh_auth_sock_warns_and_disables_passthrough() {
        let sandbox = tempfile::tempdir().unwrap();
        let socket_path = Utf8PathBuf::from_path_buf(sandbox.path().join("missing.sock")).unwrap();
        let mut warnings = Vec::new();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            |name| test_env(name, &socket_path, None),
            |_git_root, _key| panic!("git config must not be read for invalid socket"),
            |_git_root, _environment, _warning| {
                panic!("known_hosts must not be read for invalid socket")
            },
            |warning| warnings.push(warning),
        );

        assert!(passthrough.commit_signing.agent_socket_mount.is_none());
        assert!(passthrough.commit_signing.env.is_empty());
        assert!(passthrough.known_hosts.prepared.is_none());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("SSH_AUTH_SOCK does not reference a usable Unix socket"));
    }

    #[cfg(unix)]
    #[test]
    fn git_config_env_contains_only_minimal_signing_keys() {
        let (_sandbox, socket_path, _listener) = bind_test_socket();
        let mut requested_keys = Vec::new();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            |name| test_env(name, &socket_path, Some(Utf8Path::new("/home/alice"))),
            |_git_root, key| {
                requested_keys.push(key.to_string());
                Ok(match key {
                    "user.name" => Some("Alice Agent".to_string()),
                    "user.email" => Some("alice@example.test".to_string()),
                    "gpg.format" => Some("ssh".to_string()),
                    "user.signingkey" => Some("ssh-ed25519 AAAATEST alice".to_string()),
                    "commit.gpgsign" => Some("true".to_string()),
                    _ => Some("must-not-be-read".to_string()),
                })
            },
            |_git_root, _environment, _warning| None,
            panic_warning,
        );

        assert_eq!(requested_keys, GIT_CONFIG_KEYS);
        assert_eq!(
            passthrough
                .commit_signing
                .env
                .get(GIT_CONFIG_COUNT_ENV)
                .map(String::as_str),
            Some("5")
        );
        assert_git_config_env(
            &passthrough.commit_signing.env,
            0,
            "user.name",
            "Alice Agent",
        );
        assert_git_config_env(
            &passthrough.commit_signing.env,
            1,
            "user.email",
            "alice@example.test",
        );
        assert_git_config_env(&passthrough.commit_signing.env, 2, "gpg.format", "ssh");
        assert_git_config_env(
            &passthrough.commit_signing.env,
            3,
            "user.signingkey",
            "ssh-ed25519 AAAATEST alice",
        );
        assert_git_config_env(&passthrough.commit_signing.env, 4, "commit.gpgsign", "true");
        assert!(
            !passthrough
                .commit_signing
                .env
                .values()
                .any(|value| value == "must-not-be-read")
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_ssh_signing_config_is_not_injected_or_warned_about() {
        let (_sandbox, socket_path, _listener) = bind_test_socket();
        let mut warnings = Vec::new();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            |name| test_env(name, &socket_path, None),
            |_git_root, key| {
                Ok(match key {
                    "user.name" => Some("Alice Agent".to_string()),
                    "user.email" => Some("alice@example.test".to_string()),
                    "gpg.format" => Some("openpgp".to_string()),
                    "user.signingkey" => Some("ABCDEF123456".to_string()),
                    "commit.gpgsign" => Some("true".to_string()),
                    _ => None,
                })
            },
            |_git_root, _environment, _warning| None,
            |warning| warnings.push(warning),
        );

        assert_eq!(
            passthrough
                .commit_signing
                .env
                .get(GIT_CONFIG_COUNT_ENV)
                .map(String::as_str),
            Some("2")
        );
        assert_git_config_env(
            &passthrough.commit_signing.env,
            0,
            "user.name",
            "Alice Agent",
        );
        assert_git_config_env(
            &passthrough.commit_signing.env,
            1,
            "user.email",
            "alice@example.test",
        );
        assert!(
            !passthrough
                .commit_signing
                .env
                .values()
                .any(|value| value == "openpgp")
        );
        assert!(
            !passthrough
                .commit_signing
                .env
                .values()
                .any(|value| value == "ABCDEF123456")
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn passthrough_applies_mount_and_env_to_run_spec() {
        let mut spec = RuntimeRunSpec::new(
            RuntimeCreateSpec {
                image: "image".to_string(),
                labels: BTreeMap::new(),
                mounts: Vec::new(),
                command: vec!["runtime".to_string()],
                default_env: BTreeMap::new(),
                network_enabled: true,
                published_ports: Vec::new(),
            },
            "/repo",
        );
        let passthrough = SshCommitSigningPassthrough {
            agent_socket_mount: Some(RuntimeMount::bind(
                "/tmp/agent.sock",
                CONTAINER_SSH_AUTH_SOCK,
            )),
            env: BTreeMap::from([(
                HOST_SSH_AUTH_SOCK_ENV.to_string(),
                CONTAINER_SSH_AUTH_SOCK.to_string(),
            )]),
        };

        passthrough.apply_to(&mut spec);

        assert_eq!(
            spec.create().mounts,
            vec![RuntimeMount::bind(
                "/tmp/agent.sock",
                CONTAINER_SSH_AUTH_SOCK
            )]
        );
        assert_eq!(
            spec.create()
                .default_env
                .get(HOST_SSH_AUTH_SOCK_ENV)
                .map(String::as_str),
            Some(CONTAINER_SSH_AUTH_SOCK)
        );
    }

    fn assert_git_config_env(env: &BTreeMap<String, String>, index: usize, key: &str, value: &str) {
        assert_eq!(
            env.get(&format!("GIT_CONFIG_KEY_{index}"))
                .map(String::as_str),
            Some(key)
        );
        assert_eq!(
            env.get(&format!("GIT_CONFIG_VALUE_{index}"))
                .map(String::as_str),
            Some(value)
        );
    }

    fn test_env(name: &str, socket_path: &Utf8Path, home: Option<&Utf8Path>) -> Option<OsString> {
        match name {
            HOST_SSH_AUTH_SOCK_ENV => Some(socket_path.as_os_str().to_os_string()),
            "HOME" => home.map(|home| home.as_os_str().to_os_string()),
            _ => None,
        }
    }

    fn panic_warning(warning: String) {
        panic!("unexpected warning: {warning}");
    }

    #[cfg(unix)]
    fn bind_test_socket() -> (
        tempfile::TempDir,
        Utf8PathBuf,
        std::os::unix::net::UnixListener,
    ) {
        let sandbox = tempfile::tempdir().unwrap();
        let socket_path = Utf8PathBuf::from_path_buf(sandbox.path().join("agent.sock")).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(socket_path.as_std_path()).unwrap();
        (sandbox, socket_path, listener)
    }
}
