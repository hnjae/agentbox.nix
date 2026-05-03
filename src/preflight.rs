// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::runtime::RuntimeKind;
use crate::runtime::RuntimeMount;
use crate::{Error, Result};

mod snapshot;

pub use snapshot::{
    DirenvPreflightSnapshot, HostDirectoryPreflightSnapshot, HostPreflightSnapshot,
    NixConfigPreflightSnapshot, NixCustomConfPreflightSnapshot, NixPreflightSnapshot,
    OpenCodePreflightSnapshot, PreflightSnapshot,
};

pub const NIX_DAEMON_SOCKET_PATH: &str = "/nix/var/nix/daemon-socket/socket";
pub const NIX_STORE_DESTINATION: &str = "/nix";
pub const NIX_CLIENT_DESTINATION: &str = "/usr/local/bin/nix";
pub const ETC_NIX_DESTINATION: &str = "/etc/nix";
pub const ETC_STATIC_NIX_DESTINATION: &str = "/etc/static/nix";
pub const NIX_CACHE_DESTINATION: &str = "/home/user/.cache/nix";
pub const CODEX_CONFIG_DESTINATION: &str = "/home/user/.codex";
pub const OPENCODE_CONFIG_DESTINATION: &str = "/home/user/.config/opencode";
pub const OPENCODE_DATA_DESTINATION: &str = "/home/user/.local/share/opencode";

const NIX_CUSTOM_CONF_PATH: &str = "/etc/nix/nix.custom.conf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    pub host_nix_mounts: Vec<RuntimeMount>,
    pub runtime_mounts: Vec<RuntimeMount>,
}

pub fn check_host_prerequisites(
    target_directory: Option<&Utf8Path>,
    git_root: Option<&Utf8Path>,
) -> Result<PreflightReport> {
    check_host_prerequisites_for_runtime(RuntimeKind::Opencode, target_directory, git_root)
}

pub fn check_host_prerequisites_for_runtime(
    runtime: RuntimeKind,
    target_directory: Option<&Utf8Path>,
    git_root: Option<&Utf8Path>,
) -> Result<PreflightReport> {
    check_host_prerequisites_with_snapshot(
        &PreflightSnapshot::detect(target_directory, git_root),
        target_directory,
        runtime,
    )
}

pub fn check_host_prerequisites_with_snapshot(
    snapshot: &PreflightSnapshot,
    target_directory: Option<&Utf8Path>,
    runtime: RuntimeKind,
) -> Result<PreflightReport> {
    PreflightCheck {
        snapshot,
        target_directory,
        runtime,
    }
    .run()
}

struct PreflightCheck<'a> {
    snapshot: &'a PreflightSnapshot,
    target_directory: Option<&'a Utf8Path>,
    runtime: RuntimeKind,
}

impl PreflightCheck<'_> {
    fn run(&self) -> Result<PreflightReport> {
        self.validate_host_tools()?;
        self.validate_direnv()?;
        self.validate_nix_daemon()?;
        let nix_client_source = self.nix_client_source()?;
        self.validate_nix_config()?;
        let runtime_mounts = self.runtime_mounts()?;

        Ok(PreflightReport {
            host_nix_mounts: host_nix_mounts(
                nix_client_source,
                self.snapshot.nix.config.custom_conf.needs_static_mount,
            ),
            runtime_mounts,
        })
    }

    fn validate_host_tools(&self) -> Result<()> {
        if !self.snapshot.host.has_git {
            return Err(Error::msg(
                "`git` was not found on PATH; install `git` or add it to PATH",
            ));
        }

        if !self.snapshot.host.has_podman {
            return Err(Error::msg(
                "`podman` was not found on PATH; install `podman` or add it to PATH",
            ));
        }

        Ok(())
    }

    fn validate_direnv(&self) -> Result<()> {
        if self.snapshot.host.direnv.required && !self.snapshot.host.direnv.available {
            let target = self
                .target_directory
                .map(ToString::to_string)
                .unwrap_or_else(|| ".".to_string());
            return Err(Error::msg(format!(
                "`.envrc` applies to `{target}`, but `direnv` was not found on PATH; install `direnv` or add it to PATH"
            )));
        }

        Ok(())
    }

    fn validate_nix_daemon(&self) -> Result<()> {
        if !self.snapshot.nix.has_daemon_socket {
            return Err(Error::msg(format!(
                "Missing host nix-daemon socket at: {NIX_DAEMON_SOCKET_PATH}. Mount /nix:/nix:ro."
            )));
        }

        Ok(())
    }

    fn nix_client_source(&self) -> Result<&Utf8Path> {
        self.snapshot.nix.client_source.as_deref().ok_or_else(|| {
            Error::msg(
                "`nix` was not found on PATH; install Nix or add the host `nix` client to PATH",
            )
        })
    }

    fn validate_nix_config(&self) -> Result<()> {
        let config = &self.snapshot.nix.config;

        if !config.has_etc_nix_mount {
            return Err(Error::msg(
                "Missing /etc/nix host mount. Mount /etc/nix:/etc/nix:ro so the wrapper inherits the host config and registry.",
            ));
        }

        if !config.has_readable_nix_conf {
            return Err(Error::msg(
                "Missing readable host Nix config: /etc/nix/nix.conf. Mount /etc/nix:/etc/nix:ro.",
            ));
        }

        if config.custom_conf.present && !config.custom_conf.has_readable_target {
            return Err(Error::msg(
                "Missing readable target for /etc/nix/nix.custom.conf. Mount /etc/static/nix:/etc/static/nix:ro when /etc/nix points there.",
            ));
        }

        Ok(())
    }

    fn runtime_mounts(&self) -> Result<Vec<RuntimeMount>> {
        match self.runtime {
            RuntimeKind::Opencode => Ok(vec![
                HostStateMountRequirement::xdg_or_home(
                    RuntimeKind::Opencode,
                    "OpenCode",
                    "configuration",
                    "`${XDG_CONFIG_HOME:-$HOME/.config}/opencode`",
                    &self.snapshot.host.opencode.config,
                    OPENCODE_CONFIG_DESTINATION,
                )
                .validate()?,
                HostStateMountRequirement::xdg_or_home(
                    RuntimeKind::Opencode,
                    "OpenCode",
                    "data",
                    "`${XDG_DATA_HOME:-$HOME/.local/share}/opencode`",
                    &self.snapshot.host.opencode.data,
                    OPENCODE_DATA_DESTINATION,
                )
                .validate()?,
            ]),
            RuntimeKind::Codex => Ok(vec![
                HostStateMountRequirement::home_only(
                    RuntimeKind::Codex,
                    "Codex",
                    "configuration",
                    "`${HOME}/.codex`",
                    &self.snapshot.host.codex,
                    CODEX_CONFIG_DESTINATION,
                )
                .validate()?,
            ]),
        }
    }
}

struct HostStateMountRequirement<'a> {
    runtime: RuntimeKind,
    product_name: &'static str,
    description: &'static str,
    source_expression: &'static str,
    source_lookup: HostStateSourceLookup,
    state: &'a HostDirectoryPreflightSnapshot,
    destination: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum HostStateSourceLookup {
    HomeOnly,
    XdgOrHome,
}

impl<'a> HostStateMountRequirement<'a> {
    fn home_only(
        runtime: RuntimeKind,
        product_name: &'static str,
        description: &'static str,
        source_expression: &'static str,
        state: &'a HostDirectoryPreflightSnapshot,
        destination: &'static str,
    ) -> Self {
        Self {
            runtime,
            product_name,
            description,
            source_expression,
            source_lookup: HostStateSourceLookup::HomeOnly,
            state,
            destination,
        }
    }

    fn xdg_or_home(
        runtime: RuntimeKind,
        product_name: &'static str,
        description: &'static str,
        source_expression: &'static str,
        state: &'a HostDirectoryPreflightSnapshot,
        destination: &'static str,
    ) -> Self {
        Self {
            runtime,
            product_name,
            description,
            source_expression,
            source_lookup: HostStateSourceLookup::XdgOrHome,
            state,
            destination,
        }
    }

    fn validate(self) -> Result<RuntimeMount> {
        let Some(source) = self.state.source.as_ref() else {
            return Err(self.missing_source_error());
        };

        if !self.state.exists {
            return Err(Error::msg(format!(
                "Missing host {} {} directory: {source}. Run `{}` on the host first so {} exists, then retry `agentbox run --runtime {}`.",
                self.product_name,
                self.description,
                self.runtime,
                self.source_expression,
                self.runtime,
            )));
        }

        if !self.state.is_directory {
            return Err(Error::msg(format!(
                "Host {} {} path is not a directory: {source}",
                self.product_name, self.description,
            )));
        }

        if !self.state.readable || !self.state.writable {
            return Err(Error::msg(format!(
                "Host {} {} directory is not readable and writable: {source}",
                self.product_name, self.description,
            )));
        }

        Ok(RuntimeMount::bind(source.to_string(), self.destination))
    }

    fn missing_source_error(&self) -> Error {
        match self.source_lookup {
            HostStateSourceLookup::HomeOnly => Error::msg(format!(
                "`HOME` is not set; cannot locate host {} {} directory {} for `run --runtime {}`",
                self.product_name, self.description, self.source_expression, self.runtime,
            )),
            HostStateSourceLookup::XdgOrHome => Error::msg(format!(
                "Cannot locate host {} {} directory {} for `run --runtime {}`; set `HOME` or the matching XDG environment variable, then retry.",
                self.product_name, self.description, self.source_expression, self.runtime,
            )),
        }
    }
}

fn host_nix_mounts(
    nix_client_source: &Utf8Path,
    include_static_nix_mount: bool,
) -> Vec<RuntimeMount> {
    let mut mounts = vec![RuntimeMount::read_only_bind(
        NIX_STORE_DESTINATION,
        NIX_STORE_DESTINATION,
    )];
    mounts.push(RuntimeMount::read_only_bind(
        nix_client_source.to_string(),
        NIX_CLIENT_DESTINATION,
    ));
    mounts.push(RuntimeMount::read_only_bind(
        ETC_NIX_DESTINATION,
        ETC_NIX_DESTINATION,
    ));

    if include_static_nix_mount {
        mounts.push(RuntimeMount::read_only_bind(
            ETC_STATIC_NIX_DESTINATION,
            ETC_STATIC_NIX_DESTINATION,
        ));
    }

    mounts
}

pub fn direnv_applies_to_target(target_directory: &Utf8Path, git_root: &Utf8Path) -> bool {
    envrc_applies_within_git_root(target_directory, git_root)
}

pub fn required_host_mount_destinations() -> [&'static str; 4] {
    [
        NIX_STORE_DESTINATION,
        NIX_CLIENT_DESTINATION,
        ETC_NIX_DESTINATION,
        ETC_STATIC_NIX_DESTINATION,
    ]
}

fn envrc_applies_within_git_root(target_directory: &Utf8Path, git_root: &Utf8Path) -> bool {
    if target_directory != git_root && !target_directory.starts_with(git_root) {
        return false;
    }

    target_directory
        .ancestors()
        .take_while(|candidate| *candidate != git_root)
        .chain(std::iter::once(git_root))
        .any(|candidate| candidate.join(".envrc").is_file())
}

fn symlink_or_path_exists(path: &Utf8Path) -> bool {
    fs::symlink_metadata(path.as_std_path()).is_ok()
}

fn path_reaches_mount_root(path: &Utf8Path, mount_root: &Utf8Path) -> bool {
    let mount_root = normalize_path(mount_root);
    let resolved_path = resolve_path(path);
    if is_path_or_descendant(&normalize_path(path), &mount_root)
        || is_path_or_descendant(&normalize_path(&resolved_path), &mount_root)
    {
        return true;
    }

    for ancestor in path.ancestors() {
        let Some(target) = read_symlink_target(ancestor) else {
            continue;
        };
        let target_path = resolve_symlink_target(ancestor, &target);
        let expanded_path = match path.strip_prefix(ancestor) {
            Ok(suffix) if !suffix.as_str().is_empty() => target_path.join(suffix),
            _ => target_path.clone(),
        };

        if is_path_or_descendant(&normalize_path(&target_path), &mount_root)
            || is_path_or_descendant(&normalize_path(&expanded_path), &mount_root)
        {
            return true;
        }
    }

    false
}

fn read_symlink_target(path: &Utf8Path) -> Option<Utf8PathBuf> {
    fs::read_link(path.as_std_path())
        .ok()
        .and_then(|target| Utf8PathBuf::from_path_buf(target).ok())
}

fn resolve_symlink_target(link_path: &Utf8Path, target: &Utf8Path) -> Utf8PathBuf {
    if target.is_absolute() {
        return target.to_owned();
    }

    link_path
        .parent()
        .map(|parent| parent.join(target))
        .unwrap_or_else(|| target.to_owned())
}

fn is_path_or_descendant(path: &Utf8Path, root: &Utf8Path) -> bool {
    path == root || path.starts_with(root)
}

fn normalize_path(path: &Utf8Path) -> Utf8PathBuf {
    let mut normalized = Utf8PathBuf::new();
    for component in path.components() {
        match component {
            camino::Utf8Component::Prefix(prefix) => normalized.push(prefix.as_str()),
            camino::Utf8Component::RootDir => normalized.push("/"),
            camino::Utf8Component::CurDir => {}
            camino::Utf8Component::ParentDir => {
                if !normalized.pop() && !path.is_absolute() {
                    normalized.push("..");
                }
            }
            camino::Utf8Component::Normal(part) => normalized.push(part),
        }
    }

    if normalized.as_str().is_empty() {
        Utf8PathBuf::from(".")
    } else {
        normalized
    }
}

fn resolve_path(path: &Utf8Path) -> Utf8PathBuf {
    fs::canonicalize(path.as_std_path())
        .ok()
        .and_then(|value| Utf8PathBuf::from_path_buf(value).ok())
        .unwrap_or_else(|| path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[cfg(unix)]
    #[test]
    fn path_reaches_mount_root_when_symlink_points_through_mount_root() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let etc_nix = root.join("etc/nix");
        let static_nix = root.join("etc/static/nix");
        let store = root.join("nix/store");
        fs::create_dir_all(&etc_nix).unwrap();
        fs::create_dir_all(&static_nix).unwrap();
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("nix.conf"), "sandbox = false\n").unwrap();

        symlink(
            store.join("nix.conf").as_std_path(),
            static_nix.join("nix.custom.conf").as_std_path(),
        )
        .unwrap();
        symlink(
            static_nix.join("nix.custom.conf").as_std_path(),
            etc_nix.join("nix.custom.conf").as_std_path(),
        )
        .unwrap();

        assert!(path_reaches_mount_root(
            &etc_nix.join("nix.custom.conf"),
            &static_nix,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn path_reaches_mount_root_when_parent_symlink_points_to_mount_root() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let etc = root.join("etc");
        let static_nix = root.join("etc/static/nix");
        fs::create_dir_all(&etc).unwrap();
        fs::create_dir_all(&static_nix).unwrap();
        fs::write(static_nix.join("nix.custom.conf"), "sandbox = false\n").unwrap();
        symlink(static_nix.as_std_path(), etc.join("nix").as_std_path()).unwrap();

        assert!(path_reaches_mount_root(
            &etc.join("nix/nix.custom.conf"),
            &static_nix,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn path_does_not_reach_mount_root_when_symlink_points_directly_elsewhere() {
        let sandbox = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(sandbox.path().to_path_buf()).unwrap();
        let etc_nix = root.join("etc/nix");
        let static_nix = root.join("etc/static/nix");
        let store = root.join("nix/store");
        fs::create_dir_all(&etc_nix).unwrap();
        fs::create_dir_all(&static_nix).unwrap();
        fs::create_dir_all(&store).unwrap();
        fs::write(store.join("nix.conf"), "sandbox = false\n").unwrap();
        symlink(
            store.join("nix.conf").as_std_path(),
            etc_nix.join("nix.custom.conf").as_std_path(),
        )
        .unwrap();

        assert!(!path_reaches_mount_root(
            &etc_nix.join("nix.custom.conf"),
            &static_nix,
        ));
    }
}
