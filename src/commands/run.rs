// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::cli::RunArgs;
use crate::diagnostic;
use crate::{Error, Result};

use super::container_launch::{ServerLaunchMode, prepare_server_launch};
use super::detached_server::{DetachedServerLifecycle, launch_detached_server};
use super::launch_policy::{
    CommandInterrupt, ContainerLogContext, error_with_container_logs, select_runtime,
};
use super::runtime_command::run_host_runtime_client_status;
use super::server_readiness::ServerEndpointContext;
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
        let preparation = prepare_server_launch(
            &locked,
            runtime,
            args.dev_env,
            ServerLaunchMode::TransientServer,
        )?;

        let transient = TransientRun::new(podman, workspace);
        let ready_server = launch_detached_server(
            podman,
            workspace,
            runtime,
            &preparation.run_spec,
            TransientServerLifecycle { transient },
        )?;
        let endpoint = ready_server.endpoint();

        diagnostic::info(format!(
            "transient container `{}` for `{}` is ready at `{endpoint}`; connecting",
            workspace.container_name, workspace.canonical_git_root,
        ));
        let status =
            run_host_runtime_client_status(runtime, endpoint, workspace.canonical_target.as_ref());
        transient.finish_host_client_run(runtime, endpoint, status)
    })?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct TransientServerLifecycle<'a> {
    transient: TransientRun<'a>,
}

impl DetachedServerLifecycle for TransientServerLifecycle<'_> {
    fn command_name(&self) -> &'static str {
        "run"
    }

    fn launch_description(&self) -> &'static str {
        "transient container"
    }

    fn readiness_context(&self) -> ServerEndpointContext {
        ServerEndpointContext::TransientRunContainer
    }

    fn check_interrupted(&self, interrupt: &CommandInterrupt) -> Result<()> {
        self.transient.check_interrupted(interrupt)
    }

    fn run_detached_error(&self, error: Error) -> Error {
        Error::msg(format!(
            "failed to start transient run container `{}` for `{}`: {error}",
            self.transient.workspace().container_name,
            self.transient.workspace().canonical_git_root,
        ))
    }

    fn readiness_error(&self, error: Error) -> Error {
        let workspace = self.transient.workspace();
        let error = error_with_container_logs(
            self.transient.podman(),
            workspace,
            ContainerLogContext::TransientRun,
            error,
        );
        self.transient.with_cleanup_result(error)
    }
}
