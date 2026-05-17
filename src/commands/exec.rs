// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::IsTerminal;

use crate::cli::ExecArgs;
use crate::diagnostic;
use crate::runtime::RuntimeKind;
use crate::{Error, Result};

use super::container_launch::prepare_foreground_launch;
use super::launch_policy::exit_code;
use super::runtime_command::codex_exec_runtime_command;
use super::workspace_flow::with_locked_workspace;

pub fn run(args: ExecArgs, verbose: bool) -> Result<()> {
    let runtime = RuntimeKind::Codex;

    with_locked_workspace(&args.directory, verbose, |locked| {
        let workspace = locked.workspace();
        let podman = locked.podman();
        let codex_args = args.codex_args;
        let preparation = prepare_foreground_launch(&locked, runtime, args.dev_env, |dev_env| {
            codex_exec_runtime_command(workspace.canonical_target.as_ref(), dev_env, codex_args)
        })?;
        let run_spec = preparation.run_spec;

        diagnostic::info(format!(
            "starting foreground container `{}` for Codex exec",
            workspace.container_name
        ));
        let status = podman.run_foreground(&workspace.container_name, &run_spec, use_tty())?;
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
