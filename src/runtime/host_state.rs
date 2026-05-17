// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use camino::Utf8PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeHostStateMount {
    pub(crate) source: RuntimeHostStateSource,
    pub(crate) product_name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) destination: &'static str,
}

impl RuntimeHostStateMount {
    pub(crate) fn source_expression(self) -> String {
        self.source.expression()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeHostStateSource {
    HomeOnly {
        home_relative_components: &'static [&'static str],
    },
    XdgOrHome {
        xdg_variable: &'static str,
        xdg_relative_components: &'static [&'static str],
        home_relative_components: &'static [&'static str],
    },
}

impl RuntimeHostStateSource {
    pub(crate) fn resolve(
        self,
        mut environment: impl FnMut(&str) -> Option<PathBuf>,
    ) -> Option<Utf8PathBuf> {
        match self {
            Self::HomeOnly {
                home_relative_components,
            } => resolve_home_source(&mut environment, home_relative_components),
            Self::XdgOrHome {
                xdg_variable,
                xdg_relative_components,
                home_relative_components,
            } => environment(xdg_variable)
                .and_then(|base| utf8_join(base, xdg_relative_components))
                .or_else(|| resolve_home_source(&mut environment, home_relative_components)),
        }
    }

    fn expression(self) -> String {
        match self {
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

        assert_eq!(
            xdg_source.resolve(env_lookup([
                ("HOME", "/home/example"),
                ("XDG_CONFIG_HOME", "/tmp/config"),
            ])),
            Some(Utf8PathBuf::from("/tmp/config/opencode"))
        );
        assert_eq!(
            xdg_source.resolve(env_lookup([("HOME", "/home/example")])),
            Some(Utf8PathBuf::from("/home/example/.config/opencode"))
        );
        assert_eq!(
            home_source.resolve(env_lookup([("HOME", "/home/example")])),
            Some(Utf8PathBuf::from("/home/example/.codex"))
        );
        assert_eq!(home_source.resolve(env_lookup([])), None);
    }

    fn env_lookup<const N: usize>(
        values: [(&'static str, &'static str); N],
    ) -> impl FnMut(&str) -> Option<PathBuf> {
        let values = BTreeMap::from(values);
        move |variable| values.get(variable).map(PathBuf::from)
    }
}
