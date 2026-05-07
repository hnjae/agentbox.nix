// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cli::RunArgs;
use crate::diagnostic;
use crate::metadata::runtime_package_version_label;
use crate::podman::Podman;
use crate::preflight::check_host_prerequisites_for_runtime;
use crate::prompt;
use crate::runtime::RuntimeCreateSpec;
use crate::runtime::RuntimeKind;
use crate::session::{
    classify_create_error_or_else, existing_session_error, select_single_session,
};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::container_cleanup::{ContainerCleanupFailure, ManagedContainerCleanup};
use super::runtime::ensure_default_runtime_image;
use super::runtime_command::server_runtime_command;
use super::server_readiness::{ServerEndpointWait, wait_for_server_endpoint};
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

        let runtime_version = ensure_default_runtime_image(
            podman,
            runtime,
            workspace.canonical_git_root.as_ref(),
            diagnostic::info,
        )?;
        let server_run = server_runtime_command(
            runtime,
            workspace.canonical_target.as_ref(),
            workspace.canonical_git_root.as_ref(),
        );
        let mut run_spec = runtime.create_spec(
            workspace,
            &preflight.host_nix_mounts,
            &preflight.runtime_mounts,
            server_run.argv,
        );
        if let Some(version) = runtime_version {
            run_spec
                .labels
                .insert(runtime_package_version_label(runtime), version);
        }

        let cache_volume_existed_before = podman.volume_exists(&workspace.container_name)?;
        let interrupt = RunInterrupt::install()?;

        diagnostic::info(format!(
            "starting container `{}` for `{}`",
            workspace.container_name, runtime
        ));
        match podman.run_detached(
            &workspace.container_name,
            &run_spec,
            Some(server_run.workdir.as_str()),
        ) {
            Ok(()) if interrupt.interrupted() => {
                return Err(cleanup_interrupted_run(
                    podman,
                    workspace,
                    cache_volume_existed_before,
                ));
            }
            Ok(()) => {}
            Err(_) if interrupt.interrupted() => {
                return Err(cleanup_interrupted_run(
                    podman,
                    workspace,
                    cache_volume_existed_before,
                ));
            }
            Err(error) => {
                return Err(classify_run_create_error(
                    podman, workspace, &run_spec, error,
                ));
            }
        }

        diagnostic::info(format!("waiting for `{runtime}` runtime server"));
        let endpoint = match wait_for_server_endpoint(podman, workspace, runtime, || {
            interrupt.interrupted()
        }) {
            Ok(ServerEndpointWait::Ready(_)) if interrupt.interrupted() => {
                return Err(cleanup_interrupted_run(
                    podman,
                    workspace,
                    cache_volume_existed_before,
                ));
            }
            Ok(ServerEndpointWait::Ready(endpoint)) => endpoint,
            Ok(ServerEndpointWait::Interrupted) => {
                return Err(cleanup_interrupted_run(
                    podman,
                    workspace,
                    cache_volume_existed_before,
                ));
            }
            Err(error) => return Err(error_with_container_logs(podman, workspace, error)),
        };

        if interrupt.interrupted() {
            return Err(cleanup_interrupted_run(
                podman,
                workspace,
                cache_volume_existed_before,
            ));
        }

        diagnostic::info(format!(
            "managed session `{}` for `{}` is ready at `{endpoint}`; use `agentbox attach {}` to connect",
            workspace.container_name, workspace.canonical_git_root, workspace.requested_target,
        ));

        drop(interrupt);
        Ok(())
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

fn classify_run_create_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    create_spec: &RuntimeCreateSpec,
    original_error: Error,
) -> Error {
    let wrapped = Error::runtime_command_failed(
        workspace.canonical_git_root.as_ref(),
        &workspace.container_name,
        "run the runtime server command",
        &original_error.to_string(),
    );
    classify_create_error_or_else(podman, workspace, create_spec, wrapped, |error| {
        error_with_container_logs(podman, workspace, error)
    })
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
                    "{original_error}\n\ncontainer `{container_name}` produced no logs; inspect it with `{command}`"
                ))
            } else {
                Error::msg(format!(
                    "{original_error}\n\ncontainer logs (`{command}`):\n{logs}"
                ))
            }
        }
        Err(log_error) => Error::msg(format!(
            "{original_error}\n\nfailed to read container logs with `{command}`: {log_error}"
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

fn cleanup_interrupted_run(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    cache_volume_existed_before: bool,
) -> Error {
    let cleanup = InterruptedRunCleanup::run(podman, workspace, cache_volume_existed_before);
    Error::msg(cleanup.render(workspace, cache_volume_existed_before))
}

#[derive(Debug, Default)]
struct InterruptedRunCleanup {
    failures: Vec<String>,
    cache_volume_removed: bool,
}

impl InterruptedRunCleanup {
    fn run(
        podman: &Podman,
        workspace: &WorkspaceIdentity,
        cache_volume_existed_before: bool,
    ) -> Self {
        let mut cleanup = Self::default();
        let container_name = &workspace.container_name;
        let container_cleanup = ManagedContainerCleanup::stop_and_verify(podman, container_name);

        if let Some(error) = container_cleanup.stop_error() {
            cleanup
                .failures
                .push(format!("container stop failed: {error}"));
        }

        if container_cleanup.container_removed() {
            if !cache_volume_existed_before {
                match podman.remove_volume(container_name) {
                    Ok(()) => cleanup.cache_volume_removed = true,
                    Err(error) => cleanup
                        .failures
                        .push(format!("cache volume removal failed: {error}")),
                }
            }
        } else if let Some(failure) = container_cleanup.remaining_container_failure() {
            cleanup
                .failures
                .push(interrupted_container_failure_message(failure));
            if !cache_volume_existed_before {
                cleanup
                    .failures
                    .push(interrupted_cache_volume_skip_message(failure).to_string());
            }
        }

        cleanup
    }

    fn render(&self, workspace: &WorkspaceIdentity, cache_volume_existed_before: bool) -> String {
        let mut message = format!(
            "run interrupted before managed session `{}` for `{}` became ready",
            workspace.container_name, workspace.canonical_git_root,
        );

        if self.failures.is_empty() {
            let volume_detail = if cache_volume_existed_before {
                format!(
                    "preserved existing cache volume `{}`",
                    workspace.container_name
                )
            } else if self.cache_volume_removed {
                format!(
                    "removed newly-created cache volume `{}`",
                    workspace.container_name
                )
            } else {
                format!(
                    "no new cache volume `{}` remained",
                    workspace.container_name
                )
            };

            message.push_str(&format!(
                "; cleaned up managed container `{}` and {volume_detail}; default runtime image was left untouched",
                workspace.container_name,
            ));
        } else {
            message.push_str(&format!(
                "; partial cleanup failed: {}; default runtime image was left untouched",
                self.failures.join("; "),
            ));
        }

        message
    }
}

fn interrupted_container_failure_message(failure: ContainerCleanupFailure<'_>) -> String {
    match failure {
        ContainerCleanupFailure::StillExists => "container still exists after cleanup".to_string(),
        ContainerCleanupFailure::VerificationFailed(error) => {
            format!("container cleanup verification failed: {error}")
        }
    }
}

fn interrupted_cache_volume_skip_message(failure: ContainerCleanupFailure<'_>) -> &'static str {
    match failure {
        ContainerCleanupFailure::StillExists => {
            "cache volume removal skipped because the container still exists"
        }
        ContainerCleanupFailure::VerificationFailed(_) => {
            "cache volume removal skipped because container cleanup could not be verified"
        }
    }
}
