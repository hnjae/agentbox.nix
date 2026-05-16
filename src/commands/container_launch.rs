// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::Result;
use crate::cli::DevEnvMode;
use crate::dev_env::DevEnvironment;
use crate::diagnostic;
use crate::preflight::{PreflightReport, check_host_prerequisites_for_runtime};
use crate::runtime::RuntimeKind;
use crate::session::{existing_session_error, select_single_session};

use super::runtime::ensure_default_runtime_image;
use super::runtime_command::ensure_host_runtime_client_available;
use super::workspace_flow::LockedWorkspace;

#[derive(Debug)]
pub(super) struct ContainerLaunchPreparation {
    pub(super) preflight: PreflightReport,
    pub(super) dev_env: DevEnvironment,
    pub(super) runtime_image_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HostClientRequirement {
    Required,
    NotRequired,
}

pub(super) fn prepare_container_launch(
    locked: &LockedWorkspace<'_>,
    runtime: RuntimeKind,
    dev_env_mode: DevEnvMode,
    host_client: HostClientRequirement,
) -> Result<ContainerLaunchPreparation> {
    let workspace = locked.workspace();

    diagnostic::info("checking workspace prerequisites");
    let preflight = check_host_prerequisites_for_runtime(
        runtime,
        Some(workspace.canonical_target.as_ref()),
        Some(workspace.canonical_git_root.as_ref()),
    )?;

    diagnostic::info("checking existing managed sessions");
    let podman = locked.podman();
    let sessions = locked.discover_sessions()?;
    if let Some(session) = select_single_session(&sessions, workspace)? {
        return Err(existing_session_error(podman, workspace, session));
    }

    let dev_env = DevEnvironment::resolve(
        dev_env_mode,
        workspace.canonical_target.as_ref(),
        workspace.canonical_git_root.as_ref(),
    )?;
    diagnostic::info(format!("selected development environment: {dev_env}"));

    if host_client == HostClientRequirement::Required {
        ensure_host_runtime_client_available(runtime)?;
    }

    let runtime_image_version = ensure_default_runtime_image(
        podman,
        runtime,
        workspace.canonical_git_root.as_ref(),
        diagnostic::info,
    )?;

    Ok(ContainerLaunchPreparation {
        preflight,
        dev_env,
        runtime_image_version,
    })
}
