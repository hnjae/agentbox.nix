// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

use crate::Result;
use crate::diagnostic;
use crate::git::Git;
use crate::runtime::{RuntimeMount, RuntimeRunSpec};

pub(crate) const CONTAINER_SSH_AUTH_SOCK: &str = "/run/agentbox/ssh-agent.sock";

const HOST_SSH_AUTH_SOCK_ENV: &str = "SSH_AUTH_SOCK";
const GIT_CONFIG_COUNT_ENV: &str = "GIT_CONFIG_COUNT";
const GIT_CONFIG_KEYS: &[&str] = &[
    "user.name",
    "user.email",
    "gpg.format",
    "user.signingkey",
    "commit.gpgsign",
];

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

pub(crate) fn apply_ssh_commit_signing_passthrough(
    run_spec: &mut RuntimeRunSpec,
    git_root: &Utf8Path,
) {
    let git = Git::new();
    detect_with(
        git_root,
        |name| std::env::var_os(name),
        |git_root, key| git.config_get(git_root, key),
        diagnostic::warning,
    )
    .apply_to(run_spec);
}

fn detect_with(
    git_root: &Utf8Path,
    mut environment: impl FnMut(&str) -> Option<OsString>,
    mut git_config: impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    mut warning: impl FnMut(String),
) -> SshCommitSigningPassthrough {
    let Some(host_socket) = environment(HOST_SSH_AUTH_SOCK_ENV) else {
        return SshCommitSigningPassthrough::default();
    };
    if host_socket.is_empty() {
        return SshCommitSigningPassthrough::default();
    }

    let Some(host_socket) = utf8_path(host_socket) else {
        warning(format!(
            "{HOST_SSH_AUTH_SOCK_ENV} is not UTF-8; SSH commit signing passthrough disabled"
        ));
        return SshCommitSigningPassthrough::default();
    };

    if let Err(reason) = validate_ssh_agent_socket(&host_socket) {
        warning(format!(
            "{HOST_SSH_AUTH_SOCK_ENV} does not reference a usable Unix socket ({reason}); SSH commit signing passthrough disabled"
        ));
        return SshCommitSigningPassthrough::default();
    }

    let home = environment("HOME").and_then(utf8_path);
    let mut env = BTreeMap::from([(
        HOST_SSH_AUTH_SOCK_ENV.to_string(),
        CONTAINER_SSH_AUTH_SOCK.to_string(),
    )]);
    let git_entries =
        read_git_config_entries(git_root, home.as_deref(), &mut git_config, &mut warning);
    append_git_config_env(&mut env, &git_entries);

    SshCommitSigningPassthrough {
        agent_socket_mount: Some(RuntimeMount::bind(
            host_socket.to_string(),
            CONTAINER_SSH_AUTH_SOCK,
        )),
        env,
    }
}

fn read_git_config_entries(
    git_root: &Utf8Path,
    home: Option<&Utf8Path>,
    git_config: &mut impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    warning: &mut impl FnMut(String),
) -> Vec<(String, String)> {
    let mut values = Vec::new();

    for key in GIT_CONFIG_KEYS {
        let value = match git_config(git_root, key) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(error) => {
                warning(format!(
                    "failed to read host Git config `{key}` for SSH commit signing passthrough: {error}"
                ));
                continue;
            }
        };
        values.push((key.to_string(), value));
    }

    let ssh_signing_configured = values
        .iter()
        .any(|(key, value)| key == "gpg.format" && value.trim() == "ssh");
    let mut entries = Vec::new();

    for (key, value) in values {
        if is_signing_config_key(&key) && !ssh_signing_configured {
            continue;
        }

        let value = if key == "user.signingkey" {
            let Some(value) = normalize_signing_key_value(&value, git_root, home, warning) else {
                continue;
            };
            value
        } else {
            value
        };

        entries.push((key, value));
    }

    entries
}

fn is_signing_config_key(key: &str) -> bool {
    matches!(key, "gpg.format" | "user.signingkey" | "commit.gpgsign")
}

fn append_git_config_env(env: &mut BTreeMap<String, String>, entries: &[(String, String)]) {
    if entries.is_empty() {
        return;
    }

    env.insert(GIT_CONFIG_COUNT_ENV.to_string(), entries.len().to_string());
    for (index, (key, value)) in entries.iter().enumerate() {
        env.insert(format!("GIT_CONFIG_KEY_{index}"), key.clone());
        env.insert(format!("GIT_CONFIG_VALUE_{index}"), value.clone());
    }
}

fn normalize_signing_key_value(
    value: &str,
    git_root: &Utf8Path,
    home: Option<&Utf8Path>,
    warning: &mut impl FnMut(String),
) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if is_ssh_public_key_literal(value) {
        return Some(value.to_string());
    }

    let Some(path) = signing_key_path(value, git_root, home) else {
        warning(format!(
            "could not resolve host Git config `user.signingkey` path `{value}`; skipping it for SSH commit signing passthrough"
        ));
        return None;
    };

    public_key_for_path(path.as_ref(), warning)
}

fn signing_key_path(
    value: &str,
    git_root: &Utf8Path,
    home: Option<&Utf8Path>,
) -> Option<Utf8PathBuf> {
    let path = if value == "~" {
        home?.to_path_buf()
    } else if let Some(rest) = value.strip_prefix("~/") {
        home?.join(rest)
    } else if value.starts_with('~') {
        return None;
    } else {
        Utf8PathBuf::from(value)
    };

    if path.is_relative() {
        Some(git_root.join(path))
    } else {
        Some(path)
    }
}

fn public_key_for_path(path: &Utf8Path, warning: &mut impl FnMut(String)) -> Option<String> {
    if path.extension() == Some("pub") {
        return read_public_key_file(path, warning);
    }

    let public_key_path = Utf8PathBuf::from(format!("{path}.pub"));
    if public_key_path.is_file() {
        return read_public_key_file(&public_key_path, warning);
    }

    warning(format!(
        "host Git config `user.signingkey` points to `{path}`; not reading possible private key and no readable `{public_key_path}` was found"
    ));
    None
}

fn read_public_key_file(path: &Utf8Path, warning: &mut impl FnMut(String)) -> Option<String> {
    let contents = match fs::read_to_string(path.as_std_path()) {
        Ok(contents) => contents,
        Err(error) => {
            warning(format!(
                "failed to read public SSH signing key `{path}` from host Git config: {error}"
            ));
            return None;
        }
    };

    let key = contents
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    if !is_ssh_public_key_literal(key) {
        warning(format!(
            "public SSH signing key file `{path}` does not contain an SSH public key literal"
        ));
        return None;
    }

    Some(key.to_string())
}

fn is_ssh_public_key_literal(value: &str) -> bool {
    let value = value.strip_prefix("key::").unwrap_or(value);
    let Some(key_type) = value.split_whitespace().next() else {
        return false;
    };

    key_type == "ssh-rsa"
        || key_type == "ssh-ed25519"
        || key_type.starts_with("ecdsa-sha2-")
        || key_type.starts_with("sk-")
}

fn utf8_path(value: OsString) -> Option<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(PathBuf::from(value)).ok()
}

#[cfg(unix)]
fn validate_ssh_agent_socket(path: &Utf8Path) -> std::result::Result<(), String> {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::net::UnixStream;

    let metadata =
        fs::metadata(path.as_std_path()).map_err(|error| format!("{}: {error}", path.as_str()))?;
    if !metadata.file_type().is_socket() {
        return Err(format!("{} is not a Unix socket", path.as_str()));
    }

    UnixStream::connect(path.as_std_path())
        .map(|_| ())
        .map_err(|error| format!("cannot connect to {}: {error}", path.as_str()))
}

#[cfg(not(unix))]
fn validate_ssh_agent_socket(path: &Utf8Path) -> std::result::Result<(), String> {
    Err(format!(
        "{} cannot be validated on this platform",
        path.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::runtime::{RuntimeCreateSpec, RuntimeRunSpec};

    #[test]
    fn unset_ssh_auth_sock_adds_no_mount_env_or_git_config() {
        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            |_| None,
            |_git_root, _key| panic!("git config must not be read without an agent socket"),
            panic_warning,
        );

        assert_eq!(passthrough, SshCommitSigningPassthrough::default());
    }

    #[cfg(unix)]
    #[test]
    fn valid_ssh_auth_sock_adds_socket_mount_and_container_env() {
        let (_sandbox, socket_path, _listener) = bind_test_socket();

        let passthrough = detect_with(
            Utf8Path::new("/repo"),
            |name| test_env(name, &socket_path, None),
            |_git_root, _key| Ok(None),
            panic_warning,
        );

        assert_eq!(
            passthrough.agent_socket_mount,
            Some(RuntimeMount::bind(
                socket_path.to_string(),
                CONTAINER_SSH_AUTH_SOCK
            ))
        );
        assert_eq!(
            passthrough
                .env
                .get(HOST_SSH_AUTH_SOCK_ENV)
                .map(String::as_str),
            Some(CONTAINER_SSH_AUTH_SOCK)
        );
        assert!(!passthrough.env.contains_key(GIT_CONFIG_COUNT_ENV));
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
            |warning| warnings.push(warning),
        );

        assert_eq!(passthrough, SshCommitSigningPassthrough::default());
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
            panic_warning,
        );

        assert_eq!(requested_keys, GIT_CONFIG_KEYS);
        assert_eq!(
            passthrough
                .env
                .get(GIT_CONFIG_COUNT_ENV)
                .map(String::as_str),
            Some("5")
        );
        assert_git_config_env(&passthrough.env, 0, "user.name", "Alice Agent");
        assert_git_config_env(&passthrough.env, 1, "user.email", "alice@example.test");
        assert_git_config_env(&passthrough.env, 2, "gpg.format", "ssh");
        assert_git_config_env(
            &passthrough.env,
            3,
            "user.signingkey",
            "ssh-ed25519 AAAATEST alice",
        );
        assert_git_config_env(&passthrough.env, 4, "commit.gpgsign", "true");
        assert!(
            !passthrough
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
            |warning| warnings.push(warning),
        );

        assert_eq!(
            passthrough
                .env
                .get(GIT_CONFIG_COUNT_ENV)
                .map(String::as_str),
            Some("2")
        );
        assert_git_config_env(&passthrough.env, 0, "user.name", "Alice Agent");
        assert_git_config_env(&passthrough.env, 1, "user.email", "alice@example.test");
        assert!(!passthrough.env.values().any(|value| value == "openpgp"));
        assert!(
            !passthrough
                .env
                .values()
                .any(|value| value == "ABCDEF123456")
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn signing_key_literal_is_preserved() {
        let mut warnings = Vec::new();
        let value = normalize_signing_key_value(
            "ssh-ed25519 AAAATEST alice",
            Utf8Path::new("/repo"),
            None,
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(value.as_deref(), Some("ssh-ed25519 AAAATEST alice"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn signing_key_public_file_path_is_converted_to_literal() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let key_path = root.join("signing.pub");
        fs::write(&key_path, "ssh-ed25519 AAAAPUBLIC alice\n").unwrap();
        let mut warnings = Vec::new();

        let value = normalize_signing_key_value(key_path.as_str(), &root, None, &mut |warning| {
            warnings.push(warning)
        });

        assert_eq!(value.as_deref(), Some("ssh-ed25519 AAAAPUBLIC alice"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn signing_key_private_file_path_uses_sibling_public_key() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let private_key_path = root.join("id_ed25519");
        fs::write(&private_key_path, "PRIVATE KEY CONTENT\n").unwrap();
        fs::write(
            Utf8PathBuf::from(format!("{private_key_path}.pub")),
            "ssh-ed25519 AAAAPUBLIC alice\n",
        )
        .unwrap();
        let mut warnings = Vec::new();

        let value =
            normalize_signing_key_value(private_key_path.as_str(), &root, None, &mut |warning| {
                warnings.push(warning)
            });

        assert_eq!(value.as_deref(), Some("ssh-ed25519 AAAAPUBLIC alice"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn signing_key_private_file_path_without_public_key_is_skipped() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let private_key_path = root.join("id_ed25519");
        fs::write(&private_key_path, "PRIVATE KEY CONTENT\n").unwrap();
        let mut warnings = Vec::new();

        let value =
            normalize_signing_key_value(private_key_path.as_str(), &root, None, &mut |warning| {
                warnings.push(warning)
            });

        assert!(value.is_none());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not reading possible private key"));
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
