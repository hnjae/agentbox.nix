// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

pub(super) fn normalize_signing_key_value(
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
