// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeHostStateMount {
    pub(crate) source: RuntimeHostStateSource,
    pub(crate) product_name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) destination: RuntimeHostStateDestination,
    pub(crate) container_environment: Option<RuntimeHostStateContainerEnvironment>,
}

impl RuntimeHostStateMount {
    pub(crate) fn source_expression(self) -> String {
        self.source.expression()
    }

    pub(crate) fn snapshot_key(self) -> &'static str {
        self.destination.snapshot_key()
    }

    pub(crate) fn destination_for(
        self,
        source: &Utf8Path,
        source_environment_variable: Option<&str>,
    ) -> Result<String> {
        self.destination
            .resolve(source, source_environment_variable)
    }

    pub(crate) fn container_environment_for(
        self,
        source: &Utf8Path,
        source_environment_variable: Option<&str>,
    ) -> Option<(String, String)> {
        self.container_environment
            .and_then(|environment| environment.entry_for(source, source_environment_variable))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeHostStateSource {
    EnvironmentOrHome {
        environment_variable: &'static str,
        home_relative_components: &'static [&'static str],
    },
    HomeOnly {
        home_relative_components: &'static [&'static str],
    },
    XdgOrHome {
        xdg_variable: &'static str,
        xdg_relative_components: &'static [&'static str],
        home_relative_components: &'static [&'static str],
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeHostStateDestination {
    Fixed(&'static str),
    SourcePathWhenEnvironment {
        environment_variable: &'static str,
        fallback_destination: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeHostStateContainerEnvironment {
    pub(crate) name: &'static str,
    pub(crate) source_environment_variable: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeHostStateSourceResolution {
    pub(crate) source: Option<Utf8PathBuf>,
    pub(crate) source_environment_variable: Option<&'static str>,
}

impl RuntimeHostStateSource {
    pub(crate) fn resolve(
        self,
        mut environment: impl FnMut(&str) -> Option<PathBuf>,
    ) -> Result<RuntimeHostStateSourceResolution> {
        match self {
            Self::EnvironmentOrHome {
                environment_variable,
                home_relative_components,
            } => Ok(environment_source(environment_variable, &mut environment)?
                .map(|source| RuntimeHostStateSourceResolution {
                    source: Some(source),
                    source_environment_variable: Some(environment_variable),
                })
                .unwrap_or_else(|| RuntimeHostStateSourceResolution {
                    source: resolve_home_source(&mut environment, home_relative_components),
                    source_environment_variable: None,
                })),
            Self::HomeOnly {
                home_relative_components,
            } => Ok(RuntimeHostStateSourceResolution {
                source: resolve_home_source(&mut environment, home_relative_components),
                source_environment_variable: None,
            }),
            Self::XdgOrHome {
                xdg_variable,
                xdg_relative_components,
                home_relative_components,
            } => Ok(RuntimeHostStateSourceResolution {
                source: environment(xdg_variable)
                    .and_then(|base| utf8_join(base, xdg_relative_components))
                    .or_else(|| resolve_home_source(&mut environment, home_relative_components)),
                source_environment_variable: None,
            }),
        }
    }

    fn expression(self) -> String {
        match self {
            Self::EnvironmentOrHome {
                environment_variable,
                home_relative_components,
            } => format!(
                "`{environment_variable}` or {}",
                shell_expression("$HOME", home_relative_components)
            ),
            Self::HomeOnly {
                home_relative_components,
            } => shell_expression("${HOME}", home_relative_components),
            Self::XdgOrHome {
                xdg_variable,
                xdg_relative_components,
                home_relative_components,
            } => xdg_or_home_shell_expression(
                xdg_variable,
                xdg_relative_components,
                home_relative_components,
            ),
        }
    }
}

impl RuntimeHostStateDestination {
    fn snapshot_key(self) -> &'static str {
        match self {
            Self::Fixed(destination) => destination,
            Self::SourcePathWhenEnvironment {
                fallback_destination,
                ..
            } => fallback_destination,
        }
    }

    fn resolve(
        self,
        source: &Utf8Path,
        source_environment_variable: Option<&str>,
    ) -> Result<String> {
        match self {
            Self::Fixed(destination) => Ok(destination.to_string()),
            Self::SourcePathWhenEnvironment {
                environment_variable,
                fallback_destination: _,
            } if source_environment_variable == Some(environment_variable) => {
                if !source.is_absolute() {
                    return Err(Error::msg(format!(
                        "`{environment_variable}` must be an absolute path for Codex server passthrough: {source}"
                    )));
                }

                Ok(source.to_string())
            }
            Self::SourcePathWhenEnvironment {
                fallback_destination,
                ..
            } => Ok(fallback_destination.to_string()),
        }
    }
}

impl RuntimeHostStateContainerEnvironment {
    fn entry_for(
        self,
        source: &Utf8Path,
        source_environment_variable: Option<&str>,
    ) -> Option<(String, String)> {
        (source_environment_variable == Some(self.source_environment_variable))
            .then(|| (self.name.to_string(), source.to_string()))
    }
}

fn xdg_or_home_shell_expression(
    xdg_variable: &str,
    xdg_relative_components: &[&str],
    home_relative_components: &[&str],
) -> String {
    if let Some(home_fallback_base_components) =
        home_relative_components.strip_suffix(xdg_relative_components)
    {
        let home_fallback_base = shell_path("$HOME", home_fallback_base_components);
        return shell_expression(
            &format!("${{{xdg_variable}:-{home_fallback_base}}}"),
            xdg_relative_components,
        );
    }

    format!(
        "{} or {}",
        shell_expression(&format!("${xdg_variable}"), xdg_relative_components),
        shell_expression("$HOME", home_relative_components),
    )
}

fn shell_expression(base: &str, components: &[&str]) -> String {
    format!("`{}`", shell_path(base, components))
}

fn shell_path(base: &str, components: &[&str]) -> String {
    std::iter::once(base)
        .chain(components.iter().copied())
        .collect::<Vec<_>>()
        .join("/")
}

fn resolve_home_source(
    environment: &mut impl FnMut(&str) -> Option<PathBuf>,
    home_relative_components: &[&str],
) -> Option<Utf8PathBuf> {
    environment("HOME").and_then(|home| utf8_join(home, home_relative_components))
}

fn environment_source(
    variable: &'static str,
    environment: &mut impl FnMut(&str) -> Option<PathBuf>,
) -> Result<Option<Utf8PathBuf>> {
    let Some(source) = environment(variable) else {
        return Ok(None);
    };
    if source.as_os_str().is_empty() {
        return Ok(None);
    }

    Utf8PathBuf::from_path_buf(source)
        .map(Some)
        .map_err(|source| {
            Error::msg(format!(
                "`{variable}` must be a UTF-8 path for Codex server passthrough: {}",
                source.display()
            ))
        })
}

fn utf8_join(base: PathBuf, components: &[&str]) -> Option<Utf8PathBuf> {
    let mut path = Utf8PathBuf::from_path_buf(base).ok()?;
    for component in components {
        path.push(component);
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn runtime_host_state_sources_format_home_only_expression() {
        let source = RuntimeHostStateSource::HomeOnly {
            home_relative_components: &[".codex"],
        };

        assert_eq!(source.expression(), "`${HOME}/.codex`");
    }

    #[test]
    fn runtime_host_state_sources_format_environment_with_home_fallback_expression() {
        let source = RuntimeHostStateSource::EnvironmentOrHome {
            environment_variable: "CODEX_HOME",
            home_relative_components: &[".codex"],
        };

        assert_eq!(source.expression(), "`CODEX_HOME` or `$HOME/.codex`");
    }

    #[test]
    fn runtime_host_state_sources_format_xdg_with_home_suffix_fallback() {
        let source = RuntimeHostStateSource::XdgOrHome {
            xdg_variable: "XDG_CONFIG_HOME",
            xdg_relative_components: &["opencode"],
            home_relative_components: &[".config", "opencode"],
        };

        assert_eq!(
            source.expression(),
            "`${XDG_CONFIG_HOME:-$HOME/.config}/opencode`"
        );
    }

    #[test]
    fn runtime_host_state_sources_format_xdg_with_distinct_home_fallback() {
        let source = RuntimeHostStateSource::XdgOrHome {
            xdg_variable: "XDG_EXAMPLE_HOME",
            xdg_relative_components: &["example", "state"],
            home_relative_components: &[".example"],
        };

        assert_eq!(
            source.expression(),
            "`$XDG_EXAMPLE_HOME/example/state` or `$HOME/.example`"
        );
    }

    #[test]
    fn runtime_host_state_sources_resolve_from_environment() {
        let xdg_source = RuntimeHostStateSource::XdgOrHome {
            xdg_variable: "XDG_CONFIG_HOME",
            xdg_relative_components: &["opencode"],
            home_relative_components: &[".config", "opencode"],
        };
        let home_source = RuntimeHostStateSource::HomeOnly {
            home_relative_components: &[".codex"],
        };
        let environment_source = RuntimeHostStateSource::EnvironmentOrHome {
            environment_variable: "CODEX_HOME",
            home_relative_components: &[".codex"],
        };

        assert_eq!(
            xdg_source
                .resolve(env_lookup([
                    ("HOME", "/home/example"),
                    ("XDG_CONFIG_HOME", "/tmp/config"),
                ]))
                .unwrap()
                .source,
            Some(Utf8PathBuf::from("/tmp/config/opencode"))
        );
        assert_eq!(
            xdg_source
                .resolve(env_lookup([("HOME", "/home/example")]))
                .unwrap()
                .source,
            Some(Utf8PathBuf::from("/home/example/.config/opencode"))
        );
        assert_eq!(
            home_source
                .resolve(env_lookup([("HOME", "/home/example")]))
                .unwrap()
                .source,
            Some(Utf8PathBuf::from("/home/example/.codex"))
        );
        assert_eq!(
            environment_source
                .resolve(env_lookup([
                    ("HOME", "/home/example"),
                    ("CODEX_HOME", "/custom/codex"),
                ]))
                .unwrap(),
            RuntimeHostStateSourceResolution {
                source: Some(Utf8PathBuf::from("/custom/codex")),
                source_environment_variable: Some("CODEX_HOME"),
            }
        );
        assert_eq!(
            environment_source
                .resolve(env_lookup([("HOME", "/home/example")]))
                .unwrap(),
            RuntimeHostStateSourceResolution {
                source: Some(Utf8PathBuf::from("/home/example/.codex")),
                source_environment_variable: None,
            }
        );
        assert_eq!(home_source.resolve(env_lookup([])).unwrap().source, None);
    }

    #[test]
    fn runtime_host_state_destinations_can_follow_environment_source_paths() {
        let destination = RuntimeHostStateDestination::SourcePathWhenEnvironment {
            environment_variable: "CODEX_HOME",
            fallback_destination: "/home/user/.codex",
        };

        assert_eq!(
            destination
                .resolve(Utf8Path::new("/custom/codex"), Some("CODEX_HOME"))
                .unwrap(),
            "/custom/codex"
        );
        assert_eq!(
            destination
                .resolve(Utf8Path::new("/home/example/.codex"), None)
                .unwrap(),
            "/home/user/.codex"
        );
        assert_eq!(
            destination
                .resolve(Utf8Path::new("relative/codex"), Some("CODEX_HOME"))
                .unwrap_err()
                .to_string(),
            "`CODEX_HOME` must be an absolute path for Codex server passthrough: relative/codex"
        );
    }

    #[test]
    fn runtime_host_state_container_environment_uses_environment_sources_only() {
        let environment = RuntimeHostStateContainerEnvironment {
            name: "CODEX_HOME",
            source_environment_variable: "CODEX_HOME",
        };

        assert_eq!(
            environment.entry_for(Utf8Path::new("/custom/codex"), Some("CODEX_HOME")),
            Some(("CODEX_HOME".to_string(), "/custom/codex".to_string()))
        );
        assert_eq!(
            environment.entry_for(Utf8Path::new("/home/example/.codex"), None),
            None
        );
    }

    fn env_lookup<const N: usize>(
        values: [(&'static str, &'static str); N],
    ) -> impl FnMut(&str) -> Option<PathBuf> {
        let values = BTreeMap::from(values);
        move |variable| values.get(variable).map(PathBuf::from)
    }
}
