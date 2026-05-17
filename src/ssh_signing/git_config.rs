// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use camino::Utf8Path;

use crate::Result;

use super::signing_key::normalize_signing_key_value;

pub(super) const GIT_CONFIG_COUNT_ENV: &str = "GIT_CONFIG_COUNT";
pub(super) const GIT_IDENTITY_KEYS: &[&str] = &["user.name", "user.email"];
pub(super) const GIT_SIGNING_KEYS: &[&str] = &["gpg.format", "user.signingkey", "commit.gpgsign"];

pub(super) fn codex_exec_identity_entries() -> Vec<(String, String)> {
    vec![
        ("user.name".to_string(), "Codex".to_string()),
        ("user.email".to_string(), "noreply@openai.com".to_string()),
    ]
}

pub(super) fn read_git_identity_entries(
    git_root: &Utf8Path,
    git_config: &mut impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    warning: &mut impl FnMut(String),
) -> Vec<(String, String)> {
    read_git_config_keys(
        git_root,
        GIT_IDENTITY_KEYS,
        "host Git identity passthrough",
        git_config,
        warning,
    )
}

pub(super) fn read_ssh_signing_config_entries(
    git_root: &Utf8Path,
    home: Option<&Utf8Path>,
    git_config: &mut impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    warning: &mut impl FnMut(String),
) -> Vec<(String, String)> {
    let values = read_git_config_keys(
        git_root,
        GIT_SIGNING_KEYS,
        "SSH commit signing passthrough",
        git_config,
        warning,
    );

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

fn read_git_config_keys(
    git_root: &Utf8Path,
    keys: &[&str],
    context: &str,
    git_config: &mut impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    warning: &mut impl FnMut(String),
) -> Vec<(String, String)> {
    let mut values = Vec::new();

    for key in keys {
        let value = match git_config(git_root, key) {
            Ok(Some(value)) => value,
            Ok(None) => continue,
            Err(error) => {
                warning(format!(
                    "failed to read host Git config `{key}` for {context}: {error}"
                ));
                continue;
            }
        };
        values.push((key.to_string(), value));
    }

    values
}

pub(super) fn append_git_config_env(
    env: &mut BTreeMap<String, String>,
    entries: &[(String, String)],
) {
    if entries.is_empty() {
        return;
    }

    env.insert(GIT_CONFIG_COUNT_ENV.to_string(), entries.len().to_string());
    for (index, (key, value)) in entries.iter().enumerate() {
        env.insert(format!("GIT_CONFIG_KEY_{index}"), key.clone());
        env.insert(format!("GIT_CONFIG_VALUE_{index}"), value.clone());
    }
}

fn is_signing_config_key(key: &str) -> bool {
    matches!(key, "gpg.format" | "user.signingkey" | "commit.gpgsign")
}
