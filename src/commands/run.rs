// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cli::RunArgs;
use crate::diagnostic;
use crate::runtime::RuntimeRunMode;
use crate::{Error, Result};

use super::container_launch::{HostClientRequirement, prepare_container_launch};
use super::launch_policy::{
    CommandInterrupt, ContainerLogContext, error_with_container_logs, select_runtime,
};
use super::runtime_command::{run_host_runtime_client_status, server_runtime_command};
use super::server_readiness::{ServerEndpointWait, wait_for_transient_server_endpoint};
use super::transient_run::TransientRun;
use super::workspace_flow::with_locked_workspace;

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let runtime = select_runtime(
        args.runtime,
        "agentbox run requires --runtime when stdin or stderr is not a TTY",
    )?;

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        let podman = locked.podman();
        let preparation = prepare_container_launch(
            &locked,
            runtime,
            args.dev_env,
            HostClientRequirement::Required,
        )?;
        let server_run = server_runtime_command(
            runtime,
            workspace.canonical_target.as_ref(),
            &preparation.dev_env,
        );
        let run_spec = runtime.run_spec(
            RuntimeRunMode::TransientServer,
            workspace,
            &preparation.preflight.host_nix_mounts,
            &preparation.preflight.runtime_mounts,
            server_run,
        );

        let interrupt = CommandInterrupt::install("run")?;
        let transient = TransientRun::new(podman, workspace);
        diagnostic::info(format!(
            "starting transient container `{}` for `{}`",
            workspace.container_name, runtime
        ));
        if let Err(error) = podman.run_detached(&workspace.container_name, &run_spec) {
            if interrupt.interrupted() {
                return Err(transient.interrupted_error());
            }

            return Err(Error::msg(format!(
                "failed to start transient run container `{}` for `{}`: {error}",
                workspace.container_name, workspace.canonical_git_root,
            )));
        }
        transient.check_interrupted(&interrupt)?;

        diagnostic::info(format!("waiting for `{runtime}` runtime server"));
        let endpoint = match wait_for_transient_server_endpoint(podman, workspace, runtime, || {
            interrupt.interrupted()
        }) {
            Ok(ServerEndpointWait::Ready(endpoint)) => endpoint,
            Ok(ServerEndpointWait::Interrupted) => {
                return Err(transient.interrupted_error());
            }
            Err(error) => {
                let error = error_with_container_logs(
                    podman,
                    workspace,
                    ContainerLogContext::TransientRun,
                    error,
                );
                return Err(transient.with_cleanup_result(error));
            }
        };
        transient.check_interrupted(&interrupt)?;

        diagnostic::info(format!(
            "transient container `{}` for `{}` is ready at `{endpoint}`; connecting",
            workspace.container_name, workspace.canonical_git_root,
        ));
        let status =
            run_host_runtime_client_status(runtime, &endpoint, workspace.canonical_target.as_ref());
        transient.finish_host_client_run(runtime, &endpoint, status)
    })?;
    Ok(())
}
