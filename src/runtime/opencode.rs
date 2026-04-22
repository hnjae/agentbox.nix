// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

use crate::preflight::{
    ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION, NIX_CACHE_DESTINATION, NIX_CLIENT_DESTINATION,
    NIX_STORE_DESTINATION, PreflightReport,
};
use crate::runtime::{RuntimeCreateSpec, RuntimeExecSpec, RuntimeMount, RuntimeMountKind};
use crate::session::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED,
    LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE,
};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

pub const RUNTIME_NAME: &str = "opencode";
pub const DEFAULT_IMAGE: &str = "localhost/agentbox-opencode:local";
pub const SERVER_HOST: &str = "127.0.0.1";
pub const SERVER_PORT: u16 = 4096;
const PACKAGED_CONTAINER_EXAMPLE_RELATIVE: &str = "share/agentbox/container-example";

#[derive(Debug, Clone, Default)]
pub struct OpencodeRuntime;

impl OpencodeRuntime {
    pub fn new() -> Self {
        Self
    }

    pub fn create_spec(
        &self,
        workspace: &WorkspaceIdentity,
        image: Option<&str>,
        preflight: &PreflightReport,
    ) -> RuntimeCreateSpec {
        let image = image.unwrap_or(DEFAULT_IMAGE).to_string();
        let mut labels = BTreeMap::new();
        labels.insert(LABEL_MANAGED.to_string(), LABEL_MANAGED_VALUE.to_string());
        labels.insert(LABEL_SCHEMA.to_string(), LABEL_SCHEMA_VALUE.to_string());
        labels.insert(
            LABEL_GIT_ROOT.to_string(),
            workspace.canonical_git_root.to_string(),
        );
        labels.insert(LABEL_GIT_ROOT_HASH.to_string(), workspace.hash12.clone());
        labels.insert(LABEL_RUNTIME.to_string(), RUNTIME_NAME.to_string());
        labels.insert(LABEL_IMAGE.to_string(), image.clone());
        labels.insert(
            LABEL_LOGICAL_NAME.to_string(),
            workspace.container_name.clone(),
        );

        let mut mounts = vec![RuntimeMount {
            kind: RuntimeMountKind::Bind,
            source: workspace.canonical_git_root.to_string(),
            destination: workspace.canonical_git_root.to_string(),
            read_only: false,
        }];
        mounts.push(RuntimeMount {
            kind: RuntimeMountKind::Volume,
            source: workspace.container_name.clone(),
            destination: NIX_CACHE_DESTINATION.to_string(),
            read_only: false,
        });
        mounts.extend(preflight.host_nix_mounts.iter().cloned());

        RuntimeCreateSpec {
            image,
            labels,
            mounts,
            command: self.foreground_command().argv,
            default_env: BTreeMap::new(),
            network_enabled: true,
            published_ports: Vec::new(),
        }
    }

    pub fn foreground_command(&self) -> RuntimeExecSpec {
        RuntimeExecSpec {
            argv: ["opencode"]
                .into_iter()
                .map(|value| value.to_string())
                .collect(),
            detached: false,
        }
    }

    pub fn detached_server_start(&self) -> RuntimeExecSpec {
        RuntimeExecSpec {
            argv: [
                "/entrypoint",
                "opencode",
                "serve",
                "--hostname",
                SERVER_HOST,
                "--port",
                "4096",
            ]
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
            detached: true,
        }
    }

    pub fn health_probe(&self) -> RuntimeExecSpec {
        RuntimeExecSpec {
            argv: [
                "/entrypoint",
                "curl",
                "--max-time",
                "2",
                "-sf",
                "http://127.0.0.1:4096/global/health",
            ]
            .into_iter()
            .map(|value| value.to_string())
            .collect(),
            detached: false,
        }
    }

    pub fn attach_command(&self, target_directory: &Utf8Path) -> RuntimeExecSpec {
        RuntimeExecSpec {
            argv: vec![
                "/entrypoint".to_string(),
                "opencode".to_string(),
                "attach".to_string(),
                format!("http://{SERVER_HOST}:{SERVER_PORT}"),
                "--dir".to_string(),
                target_directory.to_string(),
            ],
            detached: false,
        }
    }

    pub fn default_image_context_dir(&self) -> Result<Utf8PathBuf> {
        resolve_container_example_dir()
    }
}

pub fn required_host_mount_destinations() -> [&'static str; 4] {
    [
        NIX_STORE_DESTINATION,
        NIX_CLIENT_DESTINATION,
        ETC_NIX_DESTINATION,
        ETC_STATIC_NIX_DESTINATION,
    ]
}

pub fn resolve_container_example_dir() -> Result<Utf8PathBuf> {
    if let Some(resolved) = resolve_container_example_dir_from(
        packaged_container_example_dir_from_current_exe().as_deref(),
        Utf8Path::new(env!("CARGO_MANIFEST_DIR")),
    ) {
        return Ok(resolved);
    }

    Err(Error::msg(format!(
        "could not locate the bundled runtime build context; expected `{PACKAGED_CONTAINER_EXAMPLE_RELATIVE}` next to the installed binary or `container-example/` under `{}`",
        env!("CARGO_MANIFEST_DIR")
    )))
}

pub fn resolve_container_example_dir_from(
    executable: Option<&Utf8Path>,
    manifest_root: &Utf8Path,
) -> Option<Utf8PathBuf> {
    executable
        .and_then(packaged_container_example_dir_for_exe)
        .or_else(|| repo_local_container_example_dir_for_manifest_root(manifest_root))
}

pub fn packaged_container_example_dir_for_exe(executable: &Utf8Path) -> Option<Utf8PathBuf> {
    let package_root = executable.parent()?.parent()?;
    let candidate = package_root.join(PACKAGED_CONTAINER_EXAMPLE_RELATIVE);
    candidate.is_dir().then_some(candidate)
}

fn packaged_container_example_dir_from_current_exe() -> Option<Utf8PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let executable = Utf8PathBuf::from_path_buf(executable).ok()?;
    Some(executable)
}

pub fn repo_local_container_example_dir_for_manifest_root(
    manifest_root: &Utf8Path,
) -> Option<Utf8PathBuf> {
    let candidate = manifest_root.join("container-example");
    candidate.is_dir().then_some(candidate)
}
