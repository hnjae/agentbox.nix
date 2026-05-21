// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::ffi::OsString;

use camino::Utf8Path;

use crate::Result;
use crate::runtime::{RuntimeMount, RuntimeRunSpec};

use super::CONTAINER_SSH_AUTH_SOCK;
use super::agent_socket::{HOST_SSH_AUTH_SOCK_ENV, detect_host_agent_socket, utf8_path};
use super::git_config::{
    append_git_config_env, codex_exec_identity_entries, read_git_identity_entries,
    read_ssh_signing_config_entries,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitIdentityPassthrough {
    Host,
    CodexExec,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SshCommitSigningPassthrough {
    agent_socket_mount: Option<RuntimeMount>,
    env: BTreeMap<String, String>,
}

impl SshCommitSigningPassthrough {
    pub(super) fn apply_to(self, run_spec: &mut RuntimeRunSpec) {
        if let Some(mount) = self.agent_socket_mount {
            run_spec.add_create_mount(mount);
        }
        run_spec.extend_create_default_env(self.env);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SshCommitSigningDetection {
    pub(super) passthrough: SshCommitSigningPassthrough,
    pub(super) host_agent_available: bool,
}

pub(super) fn detect_with(
    git_root: &Utf8Path,
    git_identity: GitIdentityPassthrough,
    environment: &mut impl FnMut(&str) -> Option<OsString>,
    git_config: &mut impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    warning: &mut impl FnMut(String),
) -> SshCommitSigningDetection {
    let mut env = BTreeMap::new();
    let mut git_entries = git_identity_entries(git_root, git_identity, git_config, warning);

    let Some(host_socket) = detect_host_agent_socket(environment, warning) else {
        append_git_config_env(&mut env, &git_entries);
        return SshCommitSigningDetection {
            passthrough: SshCommitSigningPassthrough {
                agent_socket_mount: None,
                env,
            },
            host_agent_available: false,
        };
    };

    let home = environment("HOME").and_then(utf8_path);
    env.insert(
        HOST_SSH_AUTH_SOCK_ENV.to_string(),
        CONTAINER_SSH_AUTH_SOCK.to_string(),
    );
    git_entries.extend(read_ssh_signing_config_entries(
        git_root,
        home.as_deref(),
        git_config,
        warning,
    ));
    append_git_config_env(&mut env, &git_entries);

    SshCommitSigningDetection {
        passthrough: SshCommitSigningPassthrough {
            agent_socket_mount: Some(RuntimeMount::bind(
                host_socket.to_string(),
                CONTAINER_SSH_AUTH_SOCK,
            )),
            env,
        },
        host_agent_available: true,
    }
}

fn git_identity_entries(
    git_root: &Utf8Path,
    git_identity: GitIdentityPassthrough,
    git_config: &mut impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    warning: &mut impl FnMut(String),
) -> Vec<(String, String)> {
    match git_identity {
        GitIdentityPassthrough::Host => read_git_identity_entries(git_root, git_config, warning),
        GitIdentityPassthrough::CodexExec => codex_exec_identity_entries(),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use camino::Utf8PathBuf;

    use super::super::git_config::{GIT_CONFIG_COUNT_ENV, GIT_IDENTITY_KEYS, GIT_SIGNING_KEYS};
    use super::*;
    use crate::runtime::{RuntimeCreateSpec, RuntimeRunSpec};

    #[test]
    fn unset_ssh_auth_sock_still_adds_host_git_identity() {
        let mut requested_keys = Vec::new();

        let detection = detect_with(
            Utf8Path::new("/repo"),
            GitIdentityPassthrough::Host,
            &mut |_| None,
            &mut |_git_root, key| {
                requested_keys.push(key.to_string());
                Ok(match key {
                    "user.name" => Some("Alice Agent".to_string()),
                    "user.email" => Some("alice@example.test".to_string()),
                    _ => Some("must-not-be-read".to_string()),
                })
            },
            &mut panic_warning,
        );

        assert_eq!(requested_keys, GIT_IDENTITY_KEYS);
        assert!(!detection.host_agent_available);
        assert!(detection.passthrough.agent_socket_mount.is_none());
        assert_eq!(
            detection
                .passthrough
                .env
                .get(GIT_CONFIG_COUNT_ENV)
                .map(String::as_str),
            Some("2")
        );
        assert_git_config_env(&detection.passthrough.env, 0, "user.name", "Alice Agent");
        assert_git_config_env(
            &detection.passthrough.env,
            1,
            "user.email",
            "alice@example.test",
        );
    }

    #[test]
    fn codex_exec_identity_does_not_read_host_git_identity() {
        let detection = detect_with(
            Utf8Path::new("/repo"),
            GitIdentityPassthrough::CodexExec,
            &mut |_| None,
            &mut |_git_root, _key| panic!("git config must not be read for Codex exec identity"),
            &mut panic_warning,
        );

        assert!(!detection.host_agent_available);
        assert!(detection.passthrough.agent_socket_mount.is_none());
        assert_eq!(
            detection
                .passthrough
                .env
                .get(GIT_CONFIG_COUNT_ENV)
                .map(String::as_str),
            Some("2")
        );
        assert_git_config_env(&detection.passthrough.env, 0, "user.name", "Codex");
        assert_git_config_env(
            &detection.passthrough.env,
            1,
            "user.email",
            "noreply@openai.com",
        );
    }

    #[cfg(unix)]
    #[test]
    fn valid_ssh_auth_sock_adds_socket_mount_and_container_env() {
        let (_sandbox, socket_path, _listener) = bind_test_socket();

        let detection = detect_with(
            Utf8Path::new("/repo"),
            GitIdentityPassthrough::Host,
            &mut |name| test_env(name, &socket_path, None),
            &mut |_git_root, _key| Ok(None),
            &mut panic_warning,
        );

        assert!(detection.host_agent_available);
        assert_eq!(
            detection.passthrough.agent_socket_mount,
            Some(RuntimeMount::bind(
                socket_path.to_string(),
                CONTAINER_SSH_AUTH_SOCK
            ))
        );
        assert_eq!(
            detection
                .passthrough
                .env
                .get(HOST_SSH_AUTH_SOCK_ENV)
                .map(String::as_str),
            Some(CONTAINER_SSH_AUTH_SOCK)
        );
        assert!(!detection.passthrough.env.contains_key(GIT_CONFIG_COUNT_ENV));
    }

    #[test]
    fn invalid_ssh_auth_sock_warns_and_disables_passthrough() {
        let sandbox = tempfile::tempdir().unwrap();
        let socket_path = Utf8PathBuf::from_path_buf(sandbox.path().join("missing.sock")).unwrap();
        let mut warnings = Vec::new();

        let detection = detect_with(
            Utf8Path::new("/repo"),
            GitIdentityPassthrough::Host,
            &mut |name| test_env(name, &socket_path, None),
            &mut |_git_root, _key| Ok(None),
            &mut |warning| warnings.push(warning),
        );

        assert!(!detection.host_agent_available);
        assert!(detection.passthrough.agent_socket_mount.is_none());
        assert!(detection.passthrough.env.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("SSH_AUTH_SOCK does not reference a usable Unix socket"));
    }

    #[cfg(unix)]
    #[test]
    fn git_config_env_contains_only_minimal_signing_keys() {
        let (_sandbox, socket_path, _listener) = bind_test_socket();
        let mut requested_keys = Vec::new();

        let detection = detect_with(
            Utf8Path::new("/repo"),
            GitIdentityPassthrough::Host,
            &mut |name| test_env(name, &socket_path, Some(Utf8Path::new("/home/alice"))),
            &mut |_git_root, key| {
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
            &mut panic_warning,
        );

        assert_eq!(
            requested_keys,
            GIT_IDENTITY_KEYS
                .iter()
                .chain(GIT_SIGNING_KEYS.iter())
                .copied()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            detection
                .passthrough
                .env
                .get(GIT_CONFIG_COUNT_ENV)
                .map(String::as_str),
            Some("5")
        );
        assert_git_config_env(&detection.passthrough.env, 0, "user.name", "Alice Agent");
        assert_git_config_env(
            &detection.passthrough.env,
            1,
            "user.email",
            "alice@example.test",
        );
        assert_git_config_env(&detection.passthrough.env, 2, "gpg.format", "ssh");
        assert_git_config_env(
            &detection.passthrough.env,
            3,
            "user.signingkey",
            "ssh-ed25519 AAAATEST alice",
        );
        assert_git_config_env(&detection.passthrough.env, 4, "commit.gpgsign", "true");
        assert!(
            !detection
                .passthrough
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

        let detection = detect_with(
            Utf8Path::new("/repo"),
            GitIdentityPassthrough::Host,
            &mut |name| test_env(name, &socket_path, None),
            &mut |_git_root, key| {
                Ok(match key {
                    "user.name" => Some("Alice Agent".to_string()),
                    "user.email" => Some("alice@example.test".to_string()),
                    "gpg.format" => Some("openpgp".to_string()),
                    "user.signingkey" => Some("ABCDEF123456".to_string()),
                    "commit.gpgsign" => Some("true".to_string()),
                    _ => None,
                })
            },
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(
            detection
                .passthrough
                .env
                .get(GIT_CONFIG_COUNT_ENV)
                .map(String::as_str),
            Some("2")
        );
        assert_git_config_env(&detection.passthrough.env, 0, "user.name", "Alice Agent");
        assert_git_config_env(
            &detection.passthrough.env,
            1,
            "user.email",
            "alice@example.test",
        );
        assert!(
            !detection
                .passthrough
                .env
                .values()
                .any(|value| value == "openpgp")
        );
        assert!(
            !detection
                .passthrough
                .env
                .values()
                .any(|value| value == "ABCDEF123456")
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn passthrough_applies_mount_and_env_to_run_spec() {
        let mut spec = RuntimeRunSpec::new(
            RuntimeCreateSpec::builder("image")
                .command(vec!["runtime".to_string()])
                .build(),
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
            spec.create().mounts(),
            vec![RuntimeMount::bind(
                "/tmp/agent.sock",
                CONTAINER_SSH_AUTH_SOCK
            )]
        );
        assert_eq!(
            spec.create()
                .default_env()
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
