// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use camino::Utf8Path;
use clap::Args;

use crate::dev_env::DevEnvMode;
use crate::diagnostic;
use crate::podman::Podman;
use crate::prompt;
use crate::runtime::RuntimeKind;
use crate::session::{
    RestartSessionTargetPlan, SessionDiscoveryQuery, SessionRecord, SessionTargetInput,
    prepare_restart_session,
};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};
use crate::{Error, Result};

use super::container_cleanup::ManagedContainerCleanup;
use super::container_launch::{prepare_runtime_launch, replacement_server_launch_request};
use super::launch_policy::CommandInterrupt;
use super::managed_server::{
    ManagedServerCompletion, ManagedServerCompletionKind, ManagedServerLaunch,
    ManagedServerLaunchPolicy,
};
use super::session_targets::{SessionTargetSurface, select_one_session_target, stop_prompt_label};
use super::workspace_flow::{LockedGitRoot, with_locked_git_root_verbose};

const RESTART_NON_TTY_ERROR: &str =
    "agentbox restart requires a target when stdin or stderr is not a TTY";

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RestartArgs {
    /// Development environment loading mode.
    #[arg(long = "dev-env", value_enum, default_value_t = DevEnvMode::Auto)]
    pub dev_env: DevEnvMode,

    /// Connect after the restarted session is ready.
    #[arg(short = 'c', long = "connect")]
    pub connect: bool,

    /// Workspace directory, exact orphan path, or stable session id prefix.
    #[arg(value_name = "TARGET")]
    pub target: Option<PathBuf>,
}

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
    select_one_session_target(
        SessionTargetSurface::Restart,
        "Select session to restart",
        RESTART_NON_TTY_ERROR,
        "no restartable running managed sessions exist",
        |candidate| candidate.value().to_string(),
        stop_prompt_label,
    )
    .map(SessionTargetInput::StableId)
}

fn restart_target(
    target: &SessionTargetInput,
    dev_env: DevEnvMode,
    connect: bool,
    verbose: bool,
) -> Result<()> {
    diagnostic::info(format!("resolving restart target `{}`", target.display()));
    let target_plan = RestartSessionTargetPlan::resolve(target, || {
        let podman = Podman::new();
        SessionDiscoveryQuery::agentbox_containers().discover(&podman)
    })?;

    with_locked_git_root_verbose(target_plan.lock_git_root(), verbose, |locked| {
        let session = target_plan.select_session_candidate(
            locked.git_root(),
            || locked.discover_sessions(),
            || locked.discover_agentbox_containers(),
        )?;
        let restart_session = prepare_restart_session(&target.display(), &session)?;
        let launch_workspace = restart_launch_workspace(
            &locked,
            restart_session.session(),
            restart_session.launch_directory(),
        )?;
        let runtime = restart_session.runtime();
        let preparation = prepare_runtime_launch(replacement_server_launch_request(
            locked.podman(),
            &launch_workspace,
            runtime,
            dev_env,
            connect,
        ))?;

        stop_existing_session(locked.podman(), restart_session.session())?;
        ManagedServerLaunch::new(
            locked.podman(),
            &launch_workspace,
            runtime,
            &preparation.run_spec,
            preparation.codex_attach_token.as_ref(),
            RestartServerLaunchPolicy {
                workspace: &launch_workspace,
                runtime,
            },
            ManagedServerCompletion::new(
                ManagedServerCompletionKind::Restart,
                connect,
                launch_workspace.canonical_target.as_ref(),
                launch_workspace.canonical_target.as_ref(),
            ),
        )
        .execute()
    })?;

    Ok(())
}

fn restart_launch_workspace(
    locked: &LockedGitRoot<'_>,
    session: &SessionRecord,
    launch_directory: &Utf8Path,
) -> Result<WorkspaceIdentity> {
    let workspace = resolve_workspace_identity(launch_directory).map_err(|error| {
        Error::msg(format!(
            "stored launch directory `{launch_directory}` for managed session `{}` cannot be used for restart: {error}",
            session.container_name()
        ))
    })?;

    if workspace.canonical_git_root.as_str() == locked.git_root().as_str() {
        Ok(workspace)
    } else {
        Err(Error::msg(format!(
            "stored launch directory `{}` for managed session `{}` now resolves to git root `{}` instead of `{}`; stop and start the session from the desired directory",
            launch_directory,
            session.container_name(),
            workspace.canonical_git_root,
            locked.git_root(),
        )))
    }
}

fn stop_existing_session(podman: &Podman, session: &SessionRecord) -> Result<()> {
    diagnostic::info(format!(
        "stopping managed session `{}` for restart",
        session.container_name()
    ));
    let cleanup = ManagedContainerCleanup::stop_and_verify(podman, session.container_name());

    if let Some(failure) = cleanup.remaining_failure(session.container_name()) {
        Err(Error::msg(format!(
            "failed to stop managed session `{}` for restart; replacement was not started: {}",
            session.container_name(),
            failure.render_stop_message(),
        )))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct RestartServerLaunchPolicy<'a> {
    workspace: &'a WorkspaceIdentity,
    runtime: RuntimeKind,
}

impl ManagedServerLaunchPolicy for RestartServerLaunchPolicy<'_> {
    fn command_name(&self) -> &'static str {
        "restart"
    }

    fn launch_description(&self) -> &'static str {
        "replacement container"
    }

    fn create_action(&self) -> &'static str {
        "start the replacement runtime server command"
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

    fn wrap_error(&self, error: Error) -> Error {
        restart_after_stop_error(self.workspace, self.runtime, error)
    }
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

pub type RestartPromptCandidate = prompt::Choice<String>;

pub fn restart_prompt_candidates(sessions: &[SessionRecord]) -> Vec<RestartPromptCandidate> {
    SessionTargetSurface::Restart.prompt_choices(
        sessions,
        |candidate| candidate.value().to_string(),
        stop_prompt_label,
    )
}
