// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::IsTerminal;
use std::path::PathBuf;

use crate::dev_env::DevEnvMode;
use crate::diagnostic;
use crate::runtime::RuntimeKind;
use crate::{Error, Result};
use clap::Args;

use super::container_launch::{foreground_launch_request, prepare_runtime_launch};
use super::launch_policy::exit_code;
use super::runtime_command::codex_exec_runtime_command;
use super::workspace_flow::with_locked_workspace;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct ExecArgs {
    /// Development environment loading mode.
    #[arg(long = "dev-env", value_enum, default_value_t = DevEnvMode::Auto)]
    pub dev_env: DevEnvMode,

    /// Workspace directory inside a git repository.
    pub directory: PathBuf,

    /// Arguments passed to codex exec.
    #[arg(value_name = "CODEX_EXEC_ARG", last = true)]
    pub codex_args: Vec<String>,
}

pub fn run(args: ExecArgs, verbose: bool) -> Result<()> {
    let runtime = RuntimeKind::Codex;

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        let podman = locked.podman();
        let codex_args = args.codex_args;
        let preparation = prepare_runtime_launch(foreground_launch_request(
            &locked,
            runtime,
            args.dev_env,
            |dev_env| {
                codex_exec_runtime_command(workspace.canonical_target.as_ref(), dev_env, codex_args)
            },
        ))?;

        diagnostic::info(format!(
            "starting foreground container `{}` for Codex exec",
            workspace.container_name
        ));
        let status =
            podman.run_foreground(&workspace.container_name, &preparation.run_spec, use_tty())?;
        if status.success() {
            Ok(())
        } else if let Some(code) = status.code().and_then(exit_code) {
            Err(Error::ExitCode(code))
        } else {
            Err(Error::msg(
                "foreground Codex exec container exited due to signal",
            ))
        }
    })?;
    Ok(())
}

fn use_tty() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
}
