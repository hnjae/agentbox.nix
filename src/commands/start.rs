// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use clap::Args;

use crate::Result;
use crate::dev_env::DevEnvMode;
use crate::runtime::RuntimeKind;

use super::container_launch::{managed_server_launch_request, prepare_runtime_launch};
use super::launch_policy::{CommandInterrupt, select_runtime};
use super::managed_server::{
    ManagedServerCompletion, ManagedServerCompletionKind, ManagedServerLaunch,
    ManagedServerLaunchPolicy,
};
use super::workspace_flow::with_locked_workspace;

mod interrupt;

use interrupt::InterruptedRunCleanupScope;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct StartArgs {
    /// Runtime to launch for this session.
    #[arg(long, value_enum)]
    pub runtime: Option<RuntimeKind>,

    /// Development environment loading mode.
    #[arg(long = "dev-env", value_enum, default_value_t = DevEnvMode::Auto)]
    pub dev_env: DevEnvMode,

    /// Connect after the new session is ready.
    #[arg(short = 'c', long = "connect")]
    pub connect: bool,

    /// Workspace directory inside a git repository.
    pub directory: PathBuf,

    /// Arguments passed to the runtime server.
    #[arg(value_name = "AGENT_ARG", last = true)]
    pub agent_args: Vec<String>,
}

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
            args.agent_args,
        ))?;

        let cache_volume_existed_before = podman.volume_exists(&workspace.container_name)?;
        let cleanup =
            InterruptedRunCleanupScope::new(podman, workspace, cache_volume_existed_before);
        ManagedServerLaunch::new(
            podman,
            workspace,
            runtime,
            &preparation.run_spec,
            preparation.codex_attach_token.as_ref(),
            StartServerLaunchPolicy { cleanup },
            ManagedServerCompletion::new(
                ManagedServerCompletionKind::Start,
                args.connect,
                workspace.canonical_target.as_ref(),
                workspace.requested_target.as_ref(),
            ),
        )
        .execute()
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
