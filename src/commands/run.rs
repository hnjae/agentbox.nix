// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cli::RunArgs;
use crate::dev_env::DevEnvironment;
use crate::diagnostic;
use crate::podman::Podman;
use crate::preflight::check_host_prerequisites_for_runtime;
use crate::prompt;
use crate::runtime::{AttachEndpoint, RuntimeKind};
use crate::session::{existing_session_error, select_single_session};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::container_cleanup::ManagedContainerCleanup;
use super::runtime::ensure_default_runtime_image;
use super::runtime_command::{
    ensure_host_runtime_client_available, host_client_status_error, run_host_runtime_client_status,
    server_runtime_command,
};
use super::server_readiness::{ServerEndpointWait, wait_for_transient_server_endpoint};
use super::workspace_flow::with_locked_workspace;

const RUN_FAILURE_LOG_TAIL_LINES: usize = 80;

pub fn run(args: RunArgs, verbose: bool) -> Result<()> {
    let runtime = selected_runtime(args.runtime)?;

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
        ensure_host_runtime_client_available(runtime)?;

        ensure_default_runtime_image(
            podman,
            runtime,
            workspace.canonical_git_root.as_ref(),
            diagnostic::info,
        )?;
        let server_run =
            server_runtime_command(runtime, workspace.canonical_target.as_ref(), &dev_env);
        let run_spec = runtime.transient_server_run_spec(
            workspace,
            &preflight.host_nix_mounts,
            &preflight.runtime_mounts,
            server_run,
        );

        let interrupt = RunInterrupt::install()?;
        diagnostic::info(format!(
            "starting transient container `{}` for `{}`",
            workspace.container_name, runtime
        ));
        if let Err(error) = podman.run_detached(&workspace.container_name, &run_spec) {
            if interrupt.interrupted() {
                return Err(interrupted_error(podman, workspace));
            }

            return Err(Error::msg(format!(
                "failed to start transient run container `{}` for `{}`: {error}",
                workspace.container_name, workspace.canonical_git_root,
            )));
        }
        check_interrupted(&interrupt, podman, workspace)?;

        diagnostic::info(format!("waiting for `{runtime}` runtime server"));
        let endpoint = match wait_for_transient_server_endpoint(podman, workspace, runtime, || {
            interrupt.interrupted()
        }) {
            Ok(ServerEndpointWait::Ready(endpoint)) => endpoint,
            Ok(ServerEndpointWait::Interrupted) => {
                return Err(interrupted_error(podman, workspace));
            }
            Err(error) => {
                let error = error_with_container_logs(podman, workspace, error);
                return Err(with_cleanup_result(
                    error,
                    cleanup_transient_container(podman, workspace),
                ));
            }
        };
        check_interrupted(&interrupt, podman, workspace)?;

        diagnostic::info(format!(
            "transient container `{}` for `{}` is ready at `{endpoint}`; connecting",
            workspace.container_name, workspace.canonical_git_root,
        ));
        let status =
            run_host_runtime_client_status(runtime, &endpoint, workspace.canonical_target.as_ref());
        finish_host_client_run(podman, workspace, runtime, &endpoint, status)
    })?;
    Ok(())
}

fn selected_runtime(runtime: Option<RuntimeKind>) -> Result<RuntimeKind> {
    match runtime {
        Some(runtime) => Ok(runtime),
        None => prompt::select_one(
            "Select runtime",
            RuntimeKind::variants().to_vec(),
            "agentbox run requires --runtime when stdin or stderr is not a TTY",
        ),
    }
}

fn finish_host_client_run(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    status: Result<ExitStatus>,
) -> Result<()> {
    let cleanup = cleanup_transient_container(podman, workspace);
    match status {
        Ok(status) if status.success() => cleanup,
        Ok(status) => {
            let code = status.code().and_then(exit_code);
            let error = host_client_status_error(
                runtime,
                endpoint,
                workspace.canonical_target.as_ref(),
                status,
            );
            match code {
                Some(code) => match cleanup {
                    Ok(()) => Err(Error::ExitCode(code)),
                    Err(cleanup_error) => Err(Error::ExitCodeWithMessage {
                        code,
                        message: format!("{error}; additionally, {cleanup_error}"),
                    }),
                },
                None => Err(with_cleanup_result(error, cleanup)),
            }
        }
        Err(error) => Err(with_cleanup_result(error, cleanup)),
    }
}

fn exit_code(code: i32) -> Option<u8> {
    u8::try_from(code).ok()
}

fn check_interrupted(
    interrupt: &RunInterrupt,
    podman: &Podman,
    workspace: &WorkspaceIdentity,
) -> Result<()> {
    if interrupt.interrupted() {
        Err(interrupted_error(podman, workspace))
    } else {
        Ok(())
    }
}

fn interrupted_error(podman: &Podman, workspace: &WorkspaceIdentity) -> Error {
    let error = Error::msg(format!(
        "run interrupted before transient container `{}` for `{}` finished",
        workspace.container_name, workspace.canonical_git_root,
    ));
    with_cleanup_result(error, cleanup_transient_container(podman, workspace))
}

fn cleanup_transient_container(podman: &Podman, workspace: &WorkspaceIdentity) -> Result<()> {
    diagnostic::info(format!(
        "stopping transient container `{}`",
        workspace.container_name
    ));
    let cleanup = ManagedContainerCleanup::stop_and_verify(podman, &workspace.container_name);
    if let Some(failure) = cleanup.remaining_failure(&workspace.container_name) {
        Err(Error::msg(format!(
            "failed to clean up transient run container `{}`: {}",
            workspace.container_name,
            failure.render_stop_message(),
        )))
    } else {
        Ok(())
    }
}

fn with_cleanup_result(error: Error, cleanup: Result<()>) -> Error {
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => Error::msg(format!("{error}; additionally, {cleanup_error}")),
    }
}

fn error_with_container_logs(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    original_error: Error,
) -> Error {
    let container_name = &workspace.container_name;
    let command = format!("podman logs --tail {RUN_FAILURE_LOG_TAIL_LINES} {container_name}");
    match podman.logs_tail(container_name, RUN_FAILURE_LOG_TAIL_LINES) {
        Ok(logs) => {
            let logs = logs.trim_end();
            if logs.is_empty() {
                Error::msg(format!(
                    "{original_error}\n\ntransient run container `{container_name}` produced no logs; inspect it with `{command}`"
                ))
            } else {
                Error::msg(format!(
                    "{original_error}\n\ntransient run container logs (`{command}`):\n{logs}"
                ))
            }
        }
        Err(log_error) => Error::msg(format!(
            "{original_error}\n\nfailed to read transient run container logs with `{command}`: {log_error}"
        )),
    }
}

#[derive(Debug)]
struct RunInterrupt {
    flag: Arc<AtomicBool>,
    signal_id: Option<signal_hook::SigId>,
}

impl RunInterrupt {
    fn install() -> Result<Self> {
        let flag = Arc::new(AtomicBool::new(false));
        let signal_id = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&flag))
            .map_err(|error| {
                Error::msg(format!(
                    "failed to install SIGINT cleanup handler for `agentbox run`: {error}"
                ))
            })?;

        Ok(Self {
            flag,
            signal_id: Some(signal_id),
        })
    }

    fn interrupted(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

impl Drop for RunInterrupt {
    fn drop(&mut self) {
        if let Some(signal_id) = self.signal_id.take() {
            signal_hook::low_level::unregister(signal_id);
        }
    }
}
