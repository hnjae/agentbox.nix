// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::io::IsTerminal;

use crate::cli::ExecArgs;
use crate::dev_env::DevEnvironment;
use crate::diagnostic;
use crate::preflight::check_host_prerequisites_for_runtime;
use crate::runtime::RuntimeKind;
use crate::session::{existing_session_error, select_single_session};
use crate::{Error, Result};

use super::runtime::ensure_default_runtime_image;
use super::runtime_command::codex_exec_runtime_command;
use super::workspace_flow::with_locked_workspace;

pub fn run(args: ExecArgs, verbose: bool) -> Result<()> {
    let runtime = RuntimeKind::Codex;

    with_locked_workspace(&args.directory, verbose, |locked| {
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
            args.dev_env,
            workspace.canonical_target.as_ref(),
            workspace.canonical_git_root.as_ref(),
        )?;
        diagnostic::info(format!("selected development environment: {dev_env}"));

        ensure_default_runtime_image(
            podman,
            runtime,
            workspace.canonical_git_root.as_ref(),
            diagnostic::info,
        )?;
        let codex_exec = codex_exec_runtime_command(
            workspace.canonical_target.as_ref(),
            &dev_env,
            args.codex_args,
        );
        let run_spec = runtime.foreground_run_spec(
            workspace,
            &preflight.host_nix_mounts,
            &preflight.runtime_mounts,
            codex_exec,
        );

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

fn exit_code(code: i32) -> Option<u8> {
    u8::try_from(code).ok()
}
