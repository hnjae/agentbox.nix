// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fmt;

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::DevEnvMode;
use crate::process::ProcessRunner;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevEnvironment {
    None,
    Direnv,
    Devenv {
        root: Utf8PathBuf,
    },
    NixDevelop {
        flake_root: Utf8PathBuf,
        attr: String,
    },
}

impl DevEnvironment {
    pub fn resolve(
        mode: DevEnvMode,
        target_directory: &Utf8Path,
        git_root: &Utf8Path,
    ) -> Result<Self> {
        match mode {
            DevEnvMode::None => Ok(Self::None),
            DevEnvMode::Auto => Self::resolve_auto(target_directory, git_root),
        }
    }

    pub fn wrap_argv(&self, argv: Vec<String>) -> Vec<String> {
        match self {
            Self::None => argv,
            Self::Direnv => {
                let mut wrapped = Vec::with_capacity(argv.len() + 3);
                wrapped.extend(["direnv".to_string(), "exec".to_string(), ".".to_string()]);
                wrapped.extend(argv);
                wrapped
            }
            Self::Devenv { root } => {
                let mut wrapped = Vec::with_capacity(argv.len() + 6);
                wrapped.extend([
                    "devenv".to_string(),
                    "shell".to_string(),
                    "--no-tui".to_string(),
                    "--from".to_string(),
                    path_flake_ref(root),
                    "--".to_string(),
                ]);
                wrapped.extend(argv);
                wrapped
            }
            Self::NixDevelop { flake_root, attr } => {
                let mut wrapped = Vec::with_capacity(argv.len() + 5);
                wrapped.extend([
                    "nix".to_string(),
                    "develop".to_string(),
                    "--no-write-lock-file".to_string(),
                    format!("{}#{attr}", path_flake_ref(flake_root)),
                    "--command".to_string(),
                ]);
                wrapped.extend(argv);
                wrapped
            }
        }
    }

    fn resolve_auto(target_directory: &Utf8Path, git_root: &Utf8Path) -> Result<Self> {
        if nearest_file(target_directory, git_root, ".envrc").is_some() {
            return Ok(Self::Direnv);
        }

        if let Some(devenv_nix) = nearest_file(target_directory, git_root, "devenv.nix") {
            return Ok(Self::Devenv {
                root: devenv_nix
                    .parent()
                    .expect("devenv.nix candidates always have a parent")
                    .to_path_buf(),
            });
        }

        let Some(flake_nix) = nearest_file(target_directory, git_root, "flake.nix") else {
            return Ok(Self::None);
        };
        let flake_root = flake_nix
            .parent()
            .expect("flake.nix candidates always have a parent")
            .to_path_buf();

        resolve_nix_develop(target_directory, &flake_root)
    }
}

impl fmt::Display for DevEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("none"),
            Self::Direnv => formatter.write_str("direnv"),
            Self::Devenv { root } => write!(formatter, "devenv at `{root}`"),
            Self::NixDevelop { flake_root, attr } => {
                write!(
                    formatter,
                    "nix develop `{}`#{attr}",
                    path_flake_ref(flake_root)
                )
            }
        }
    }
}

fn resolve_nix_develop(
    target_directory: &Utf8Path,
    flake_root: &Utf8Path,
) -> Result<DevEnvironment> {
    for attr in nix_develop_candidate_attrs(target_directory, flake_root) {
        if nix_dev_shell_exists(flake_root, &attr)? {
            return Ok(DevEnvironment::NixDevelop {
                flake_root: flake_root.to_path_buf(),
                attr,
            });
        }
    }

    Ok(DevEnvironment::None)
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

fn nearest_file(
    target_directory: &Utf8Path,
    git_root: &Utf8Path,
    filename: &str,
) -> Option<Utf8PathBuf> {
    ancestor_directories(target_directory, git_root)
        .into_iter()
        .map(|directory| directory.join(filename))
        .find(|candidate| candidate.is_file())
}

fn ancestor_directories(target_directory: &Utf8Path, git_root: &Utf8Path) -> Vec<Utf8PathBuf> {
    let mut directories = Vec::new();
    for candidate in target_directory.ancestors() {
        directories.push(candidate.to_path_buf());
        if candidate == git_root {
            break;
        }
    }
    directories
}

fn path_flake_ref(path: &Utf8Path) -> String {
    format!("path:{path}")
}

fn nix_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a Nix string cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_commands_preserve_server_argv_after_provider_arguments() {
        let server = vec!["opencode".to_string(), "serve".to_string()];

        assert_eq!(
            DevEnvironment::Direnv.wrap_argv(server.clone()),
            vec!["direnv", "exec", ".", "opencode", "serve"]
        );
        assert_eq!(
            DevEnvironment::Devenv {
                root: "/repo".into(),
            }
            .wrap_argv(server.clone()),
            vec![
                "devenv",
                "shell",
                "--no-tui",
                "--from",
                "path:/repo",
                "--",
                "opencode",
                "serve",
            ]
        );
        assert_eq!(
            DevEnvironment::NixDevelop {
                flake_root: "/repo".into(),
                attr: "nested".to_string(),
            }
            .wrap_argv(server),
            vec![
                "nix",
                "develop",
                "--no-write-lock-file",
                "path:/repo#nested",
                "--command",
                "opencode",
                "serve",
            ]
        );
    }

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
