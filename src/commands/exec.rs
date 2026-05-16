// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

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
