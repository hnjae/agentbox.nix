// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::process::ProcessRunner;
use crate::{Error, Result};

use super::environment::DevEnvironment;
use super::path::path_flake_ref;

pub(super) fn resolve_nix_develop(
    target_directory: &Utf8Path,
    flake_root: &Utf8Path,
    probe: &impl NixDevShellProbe,
) -> Result<DevEnvironment> {
    for attr in nix_develop_candidate_attrs(target_directory, flake_root) {
        if probe.dev_shell_exists(flake_root, &attr)? {
            return Ok(DevEnvironment::NixDevelop {
                flake_root: flake_root.to_path_buf(),
                attr,
            });
        }
    }

    Ok(DevEnvironment::None)
}

pub(super) trait NixDevShellProbe {
    fn dev_shell_exists(&self, flake_root: &Utf8Path, attr: &str) -> Result<bool>;
}

pub(super) struct HostNixDevShellProbe;

impl NixDevShellProbe for HostNixDevShellProbe {
    fn dev_shell_exists(&self, flake_root: &Utf8Path, attr: &str) -> Result<bool> {
        nix_dev_shell_exists(flake_root, attr)
    }
}

fn nix_develop_candidate_attrs(target_directory: &Utf8Path, flake_root: &Utf8Path) -> Vec<String> {
    if target_directory == flake_root {
        return vec!["default".to_string()];
    }

    let mut attrs = target_directory
        .file_name()
        .map(|name| vec![name.to_string(), "default".to_string()])
        .unwrap_or_else(|| vec!["default".to_string()]);
    attrs.dedup();
    attrs
}

fn nix_dev_shell_exists(flake_root: &Utf8Path, attr: &str) -> Result<bool> {
    let expression = nix_dev_shell_probe_expression(flake_root, attr);
    let output = ProcessRunner::new()
        .capture("nix", |command| {
            command.args([
                "eval",
                "--raw",
                "--impure",
                "--no-write-lock-file",
                "--expr",
                &expression,
            ]);
        })
        .map_err(|error| {
            Error::msg(format!(
                "failed to evaluate dev shell `{}`#{attr}: {error}",
                path_flake_ref(flake_root)
            ))
        })?;

    match output.stdout.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        value => Err(Error::msg(format!(
            "`nix eval` returned unexpected dev shell probe result `{value}` for `{}`#{attr}",
            path_flake_ref(flake_root)
        ))),
    }
}

fn nix_dev_shell_probe_expression(flake_root: &Utf8Path, attr: &str) -> String {
    let flake_ref = nix_string(&path_flake_ref(flake_root));
    let attr = nix_string(attr);
    format!(
        "let flake = builtins.getFlake {flake_ref}; system = builtins.currentSystem; devShells = flake.devShells or {{}}; shells = devShells.${{system}} or {{}}; in if builtins.hasAttr {attr} shells then \"true\" else \"false\""
    )
}

fn nix_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a Nix string cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nix_develop_parent_flake_prefers_directory_basename_then_default() {
        assert_eq!(
            nix_develop_candidate_attrs(Utf8Path::new("/repo/api"), Utf8Path::new("/repo")),
            vec!["api".to_string(), "default".to_string()]
        );
    }

    #[test]
    fn nix_develop_target_flake_uses_default_only() {
        assert_eq!(
            nix_develop_candidate_attrs(Utf8Path::new("/repo/api"), Utf8Path::new("/repo/api")),
            vec!["default".to_string()]
        );
    }
}
