// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::cli::StartArgs;

use super::container_launch::{managed_server_launch_request, prepare_runtime_launch};
use super::launch_policy::{CommandInterrupt, select_runtime};
use super::managed_server::{
    ManagedServerCompletionKind, ManagedServerLaunchPolicy, finish_managed_server_launch,
    launch_managed_server,
};
use super::workspace_flow::with_locked_workspace;

mod interrupt;

use interrupt::InterruptedRunCleanupScope;

pub fn run(args: StartArgs, verbose: bool) -> Result<()> {
    let runtime = select_runtime(
        args.runtime,
        "agentbox start requires --runtime when stdin or stderr is not a TTY",
    )?;

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        let podman = locked.podman();
        let preparation = prepare_runtime_launch(managed_server_launch_request(
            &locked,
            runtime,
            args.dev_env,
            args.connect,
        ))?;
        let run_spec = preparation.run_spec;

        let cache_volume_existed_before = podman.volume_exists(&workspace.container_name)?;
        let cleanup =
            InterruptedRunCleanupScope::new(podman, workspace, cache_volume_existed_before);
        let endpoint = launch_managed_server(
            podman,
            workspace,
            runtime,
            &run_spec,
            StartServerLaunchPolicy { cleanup },
        )?;
        finish_managed_server_launch(
            ManagedServerCompletionKind::Start,
            args.connect,
            workspace,
            runtime,
            endpoint,
            workspace.canonical_target.as_ref(),
            workspace.requested_target.as_ref(),
        )
    })?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct StartServerLaunchPolicy<'a> {
    cleanup: InterruptedRunCleanupScope<'a>,
}

impl ManagedServerLaunchPolicy for StartServerLaunchPolicy<'_> {
    fn command_name(&self) -> &'static str {
        "start"
    }

    fn launch_description(&self) -> &'static str {
        "container"
    }

    fn create_action(&self) -> &'static str {
        "start the runtime server command"
    }

    fn check_interrupted(&self, interrupt: &CommandInterrupt) -> Result<()> {
        self.cleanup.check_interrupted(interrupt)
    }
}
