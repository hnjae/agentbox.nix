// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::dev_env::DevEnvMode;
use crate::runtime::RuntimeKind;
use clap::Args;

use super::container_launch::{prepare_runtime_launch, transient_server_launch_request};
use super::launch_policy::select_runtime;
use super::transient_run::TransientRunLaunch;
use super::workspace_flow::with_locked_workspace;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RunArgs {
    /// Runtime to launch for this run.
    #[arg(long, value_enum)]
    pub runtime: Option<RuntimeKind>,

    /// Development environment loading mode.
    #[arg(long = "dev-env", value_enum, default_value_t = DevEnvMode::Auto)]
    pub dev_env: DevEnvMode,

    /// Workspace directory inside a git repository.
    pub directory: std::path::PathBuf,

    /// Arguments passed to the runtime host client.
    #[arg(value_name = "AGENT_ARG", last = true)]
    pub agent_args: Vec<String>,
}

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let runtime = select_runtime(
        args.runtime,
        "agentbox run requires --runtime when stdin or stderr is not a TTY",
    )?;

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        let podman = locked.podman();
        let preparation = prepare_runtime_launch(transient_server_launch_request(
            &locked,
            runtime,
            args.dev_env,
        ))?;
        TransientRunLaunch::new(
            podman,
            workspace,
            runtime,
            &preparation.run_spec,
            preparation.codex_attach_token.as_ref(),
            &args.agent_args,
        )
        .execute()
    })?;
    Ok(())
}
