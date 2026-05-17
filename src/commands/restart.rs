// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeSet;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

use crate::cli::RestartArgs;
use crate::diagnostic;
use crate::metadata::AgentboxContainerKind;
use crate::podman::Podman;
use crate::prompt;
use crate::runtime::{AttachEndpoint, RuntimeKind, RuntimeRunSpec};
use crate::session::{
    SessionRecord, SessionStatus, classify_create_error_or_else, discover_agentbox_containers,
    exact_git_root_matches, resource_failure_requires_action_error,
    select_agentbox_stable_id_prefix,
};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};
use crate::{Error, Result};

use super::container_cleanup::ManagedContainerCleanup;
use super::container_launch::prepare_replacement_server_launch;
use super::detached_server::{DetachedServerLifecycle, launch_detached_server};
use super::launch_policy::{CommandInterrupt, ContainerLogContext, error_with_container_logs};
use super::runtime_command::run_host_runtime_client;
use super::server_readiness::ServerEndpointContext;
use super::session_targets::SessionTargetKind;
use super::target::{ResolvedSessionTarget, SessionTargetInput, resolve_session_target};
use super::workspace_flow::{LockedGitRoot, with_locked_git_root_verbose};

pub fn run(args: RestartArgs, verbose: bool) -> Result<()> {
    let target = selected_restart_target(args.target)?;
    restart_target(&target, args.dev_env, args.connect, verbose)
}

fn selected_restart_target(target: Option<PathBuf>) -> Result<SessionTargetInput> {
    match target {
        Some(target) => Ok(SessionTargetInput::Cli(target)),
        None => select_restart_target(),
    }
}

fn select_restart_target() -> Result<SessionTargetInput> {
    prompt::require_interactive_terminal(
        "agentbox restart requires a target when stdin or stderr is not a TTY",
    )?;
    let podman = Podman::new();
    let candidates =
        restart_prompt_candidates(&crate::session::discover_managed_sessions(&podman)?);

    if candidates.is_empty() {
        return Err(Error::msg("no restartable running managed sessions exist"));
    }

    let selected = prompt::select_one(
        "Select session to restart",
        candidates,
        "agentbox restart requires a target when stdin or stderr is not a TTY",
    )?;

    Ok(SessionTargetInput::StableId(selected.into_value()))
}

fn restart_target(
    target: &SessionTargetInput,
    dev_env: crate::cli::DevEnvMode,
    connect: bool,
    verbose: bool,
) -> Result<()> {
    diagnostic::info(format!("resolving restart target `{}`", target.display()));
    let git_root = restart_lock_git_root(target)?;

    with_locked_git_root_verbose(&git_root, verbose, |locked| {
        let session = select_restartable_session_for_locked_target(&locked, target)?;
        let launch_workspace = restart_launch_workspace(&locked, &session)?;
        let runtime = session_runtime(&session)?;
        let preparation = prepare_replacement_server_launch(
            locked.podman(),
            &launch_workspace,
            runtime,
            dev_env,
            connect,
        )?;
        let run_spec = preparation.run_spec;

        stop_existing_session(locked.podman(), &session)?;
        let endpoint =
            launch_replacement_server(locked.podman(), &launch_workspace, runtime, &run_spec)?;

        finish_restart(connect, &launch_workspace, runtime, endpoint)
    })?;

    Ok(())
}

fn restart_lock_git_root(target: &SessionTargetInput) -> Result<Utf8PathBuf> {
    match resolve_session_target(target)? {
        ResolvedSessionTarget::ResolvedGitRoot(git_root)
        | ResolvedSessionTarget::ExactStoredGitRootPath(git_root) => Ok(git_root),
        ResolvedSessionTarget::StableId(prefix) => restart_lock_git_root_for_stable_id(&prefix),
    }
}

fn restart_lock_git_root_for_stable_id(prefix: &str) -> Result<Utf8PathBuf> {
    let podman = Podman::new();
    let sessions = discover_agentbox_containers(&podman)?;
    let selection = select_agentbox_stable_id_prefix(&sessions, prefix)?;
    let id = selection.id().to_string();
    let roots = selection
        .into_sessions()
        .into_iter()
        .filter_map(|session| session.canonical_git_root().map(Utf8Path::to_path_buf))
        .collect::<BTreeSet<_>>();

    match roots.len() {
        0 => Err(Error::msg(format!(
            "agentbox container id `{id}` cannot be restarted safely because no matched container has a recoverable git-root label"
        ))),
        1 => Ok(roots.into_iter().next().unwrap()),
        _ => Err(Error::msg(format!(
            "agentbox container id `{id}` matches containers with multiple git roots; cannot restart safely"
        ))),
    }
}

fn select_restartable_session_for_locked_target(
    locked: &LockedGitRoot<'_>,
    target: &SessionTargetInput,
) -> Result<SessionRecord> {
    let sessions = matching_sessions_for_locked_target(locked, target)?;
    require_single_restartable_session(sessions, target)
}

fn matching_sessions_for_locked_target(
    locked: &LockedGitRoot<'_>,
    target: &SessionTargetInput,
) -> Result<Vec<SessionRecord>> {
    match resolve_session_target(target)? {
        ResolvedSessionTarget::ResolvedGitRoot(git_root) => {
            require_locked_target_unchanged(locked.git_root(), &git_root)?;
            locked.discover_sessions()
        }
        ResolvedSessionTarget::ExactStoredGitRootPath(git_root) => {
            require_locked_target_unchanged(locked.git_root(), &git_root)?;
            Ok(exact_git_root_matches(
                locked.discover_agentbox_containers()?,
                &git_root,
            ))
        }
        ResolvedSessionTarget::StableId(prefix) => {
            let sessions = locked.discover_agentbox_containers()?;
            let selection = select_agentbox_stable_id_prefix(&sessions, &prefix)?;
            let sessions = selection
                .into_sessions()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            require_stable_id_still_matches_locked_root(locked.git_root(), &sessions)?;
            Ok(sessions)
        }
    }
}

fn require_locked_target_unchanged(locked: &Utf8Path, current: &Utf8Path) -> Result<()> {
    if locked == current {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "restart target changed from `{locked}` to `{current}` while waiting for the workspace lock; retry the command"
        )))
    }
}

fn require_stable_id_still_matches_locked_root(
    locked: &Utf8Path,
    sessions: &[SessionRecord],
) -> Result<()> {
    let matches_locked_root = sessions
        .iter()
        .any(|session| session.canonical_git_root() == Some(locked));
    if matches_locked_root {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "restart target changed away from `{locked}` while waiting for the workspace lock; retry the command"
        )))
    }
}

fn require_single_restartable_session(
    sessions: Vec<SessionRecord>,
    target: &SessionTargetInput,
) -> Result<SessionRecord> {
    match sessions.as_slice() {
        [] => Err(Error::msg(format!(
            "no running managed session matches restart target `{}`",
            target.display()
        ))),
        [_] => {
            let session = sessions.into_iter().next().unwrap();
            validate_restartable_session(&session, target)?;
            Ok(session)
        }
        _ => Err(Error::msg(format!(
            "restart target `{}` matches {} agentbox containers; restart requires exactly one running managed session. Clean up duplicates with `agentbox stop --force {}` before retrying.",
            target.display(),
            sessions.len(),
            target.display(),
        ))),
    }
}

fn validate_restartable_session(
    session: &SessionRecord,
    target: &SessionTargetInput,
) -> Result<()> {
    if session.is_transient_run() {
        return Err(Error::msg(format!(
            "transient run container `{}` cannot be restarted; stop it with `agentbox stop {}`",
            session.container_name,
            session.stable_id().unwrap_or(&session.container_name),
        )));
    }

    if !session.is_managed_session() {
        return Err(Error::msg(format!(
            "restart target `{}` is not a managed session",
            target.display()
        )));
    }

    match session.status {
        SessionStatus::Running => {
            session_runtime(session)?;
            session_launch_directory(session)?;
            Ok(())
        }
        SessionStatus::Orphaned => Err(Error::orphaned_managed_session(
            restart_session_git_root(session)?.as_ref(),
            &session.container_name,
        )),
        SessionStatus::Duplicate => Err(Error::duplicate_managed_sessions(
            restart_session_git_root(session)?.as_ref(),
        )),
        SessionStatus::Failed(Some(failure)) => Err(resource_failure_requires_action_error(
            AgentboxContainerKind::Managed,
            restart_session_git_root(session)?.as_ref(),
            &session.container_name,
            failure,
        )),
        SessionStatus::Failed(None) => Err(Error::failed_managed_session(
            restart_session_git_root(session)?.as_ref(),
            &session.container_name,
        )),
    }
}

fn restart_session_git_root(session: &SessionRecord) -> Result<Utf8PathBuf> {
    session.canonical_git_root().map(Utf8Path::to_path_buf).ok_or_else(|| {
        Error::msg(format!(
            "managed session `{}` cannot be restarted safely because it has no recoverable git-root label",
            session.container_name
        ))
    })
}

fn session_runtime(session: &SessionRecord) -> Result<RuntimeKind> {
    session.runtime_kind().ok_or_else(|| {
        Error::msg(format!(
            "managed session `{}` cannot be restarted because it has an unsupported or malformed `io.agentbox.runtime` label",
            session.container_name
        ))
    })
}

fn session_launch_directory(session: &SessionRecord) -> Result<&Utf8Path> {
    session.launch_directory().ok_or_else(|| {
        Error::msg(format!(
            "managed session `{}` cannot be restarted because it has a missing or malformed `io.agentbox.launch_directory` label",
            session.container_name
        ))
    })
}

fn restart_launch_workspace(
    locked: &LockedGitRoot<'_>,
    session: &SessionRecord,
) -> Result<WorkspaceIdentity> {
    let launch_directory = session_launch_directory(session)?;
    let workspace = resolve_workspace_identity(launch_directory).map_err(|error| {
        Error::msg(format!(
            "stored launch directory `{launch_directory}` for managed session `{}` cannot be used for restart: {error}",
            session.container_name
        ))
    })?;

    if workspace.canonical_git_root.as_str() == locked.git_root().as_str() {
        Ok(workspace)
    } else {
        Err(Error::msg(format!(
            "stored launch directory `{}` for managed session `{}` now resolves to git root `{}` instead of `{}`; stop and start the session from the desired directory",
            launch_directory,
            session.container_name,
            workspace.canonical_git_root,
            locked.git_root(),
        )))
    }
}

fn stop_existing_session(podman: &Podman, session: &SessionRecord) -> Result<()> {
    diagnostic::info(format!(
        "stopping managed session `{}` for restart",
        session.container_name
    ));
    let cleanup = ManagedContainerCleanup::stop_and_verify(podman, &session.container_name);

    if let Some(failure) = cleanup.remaining_failure(&session.container_name) {
        Err(Error::msg(format!(
            "failed to stop managed session `{}` for restart; replacement was not started: {}",
            session.container_name,
            failure.render_stop_message(),
        )))
    } else {
        Ok(())
    }
}

fn launch_replacement_server(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    run_spec: &RuntimeRunSpec,
) -> Result<AttachEndpoint> {
    launch_detached_server(
        podman,
        workspace,
        runtime,
        run_spec,
        RestartServerLifecycle {
            podman,
            workspace,
            runtime,
            run_spec,
        },
    )
    .map(|ready| ready.into_endpoint())
}

#[derive(Debug, Clone, Copy)]
struct RestartServerLifecycle<'a> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
    run_spec: &'a RuntimeRunSpec,
}

impl DetachedServerLifecycle for RestartServerLifecycle<'_> {
    fn command_name(&self) -> &'static str {
        "restart"
    }

    fn launch_description(&self) -> &'static str {
        "replacement container"
    }

    fn readiness_context(&self) -> ServerEndpointContext {
        ServerEndpointContext::ManagedSession
    }

    fn check_interrupted(&self, interrupt: &CommandInterrupt) -> Result<()> {
        if interrupt.interrupted() {
            Err(restart_after_stop_error(
                self.workspace,
                self.runtime,
                Error::msg("restart interrupted after the previous managed session was stopped"),
            ))
        } else {
            Ok(())
        }
    }

    fn run_detached_error(&self, error: Error) -> Error {
        restart_after_stop_error(
            self.workspace,
            self.runtime,
            classify_replacement_create_error(self.podman, self.workspace, self.run_spec, error),
        )
    }

    fn readiness_error(&self, error: Error) -> Error {
        restart_after_stop_error(
            self.workspace,
            self.runtime,
            error_with_container_logs(
                self.podman,
                self.workspace,
                ContainerLogContext::ManagedSession,
                error,
            ),
        )
    }
}

fn classify_replacement_create_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    run_spec: &RuntimeRunSpec,
    original_error: Error,
) -> Error {
    let wrapped = Error::runtime_command_failed(
        workspace.canonical_git_root.as_ref(),
        &workspace.container_name,
        "start the replacement runtime server command",
        &original_error.to_string(),
    );
    classify_create_error_or_else(podman, workspace, run_spec.create(), wrapped, |error| {
        error_with_container_logs(
            podman,
            workspace,
            ContainerLogContext::ManagedSession,
            error,
        )
    })
}

fn restart_after_stop_error(
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    error: Error,
) -> Error {
    Error::msg(format!(
        "{error}\n\nThe previous managed session for `{}` may already be gone because restart stopped it before starting the replacement. Retry with `agentbox start --runtime {runtime} {}` or inspect the container with Podman.",
        workspace.canonical_git_root, workspace.canonical_target,
    ))
}

fn finish_restart(
    connect: bool,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    endpoint: AttachEndpoint,
) -> Result<()> {
    if connect {
        diagnostic::info(format!(
            "managed session `{}` for `{}` restarted and ready at `{endpoint}`; connecting",
            workspace.container_name, workspace.canonical_git_root,
        ));
        run_host_runtime_client(runtime, &endpoint, workspace.canonical_target.as_ref()).map_err(
            |error| {
                Error::msg(format!(
                    "failed to connect to restarted managed session `{}` for `{}`: {error}. The session remains running; retry with `agentbox connect {}` or stop it with `agentbox stop {}`.",
                    workspace.container_name,
                    workspace.canonical_git_root,
                    workspace.canonical_target,
                    workspace.canonical_target,
                ))
            },
        )
    } else {
        diagnostic::info(format!(
            "managed session `{}` for `{}` restarted and ready at `{endpoint}`; use `agentbox connect {}` to connect",
            workspace.container_name, workspace.canonical_git_root, workspace.canonical_target,
        ));
        Ok(())
    }
}

pub type RestartPromptCandidate = prompt::Choice<String>;

pub fn restart_prompt_candidates(sessions: &[SessionRecord]) -> Vec<RestartPromptCandidate> {
    SessionTargetKind::RestartStableId.prompt_choices(
        sessions,
        |candidate| candidate.value().to_string(),
        |candidate| candidate.stop_prompt_label(),
    )
}
