// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::config::{CpuLimit, MemoryLimit, ResourceLimitOverrides, load_config};
use crate::dev_env::DevEnvMode;
use crate::diagnostic;
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

    /// CPU limit for the transient server container.
    #[arg(long)]
    pub cpus: Option<CpuLimit>,

    /// Memory limit for the transient server container.
    #[arg(long)]
    pub memory: Option<MemoryLimit>,

    /// Workspace directory inside a git repository.
    #[arg(value_name = "DIRECTORY", default_value = ".")]
    pub directory: std::path::PathBuf,

    /// Arguments passed to the runtime host client.
    #[arg(value_name = "CLIENT_ARG", last = true)]
    pub agent_args: Vec<String>,
}

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let runtime = select_runtime(
        args.runtime,
        "agentbox run requires --runtime when stdin or stderr is not a TTY",
    )?;
    let config = load_config(&mut diagnostic::warning);
    let resource_limits = config
        .default_resource_limits
        .overlay(ResourceLimitOverrides {
            cpus: args.cpus,
            memory: args.memory,
        });

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        let podman = locked.podman();
        let preparation = prepare_runtime_launch(transient_server_launch_request(
            &locked,
            runtime,
            args.dev_env,
            resource_limits,
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
