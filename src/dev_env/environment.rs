// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use camino::{Utf8Path, Utf8PathBuf};

use crate::Result;

use super::discovery::DevEnvironmentDiscovery;
use super::mode::DevEnvMode;
use super::path::path_flake_ref;

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
            Self::Devenv { .. } => {
                let mut wrapped = Vec::with_capacity(argv.len() + 4);
                wrapped.extend([
                    "devenv".to_string(),
                    "shell".to_string(),
                    "--no-tui".to_string(),
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
        DevEnvironmentDiscovery::detect(target_directory, git_root).resolve(target_directory)
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
            vec!["devenv", "shell", "--no-tui", "--", "opencode", "serve"]
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
    fn diagnostic_display_styles_only_the_selected_provider_name() {
        assert_eq!(
            DevEnvironment::None.display_with_provider_style(|provider| format!("<{provider}>")),
            "<none>"
        );
        assert_eq!(
            DevEnvironment::Devenv {
                root: "/repo".into(),
            }
            .display_with_provider_style(|provider| format!("<{provider}>")),
            "<devenv> at `/repo`"
        );
        assert_eq!(
            DevEnvironment::NixDevelop {
                flake_root: "/repo".into(),
                attr: "default".to_string(),
            }
            .display_with_provider_style(|provider| format!("<{provider}>")),
            "<nix develop> `path:/repo`#default"
        );
    }
}
