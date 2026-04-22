// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::process::{Command, Stdio};

use camino::Utf8Path;

use crate::cli::RunArgs;
use crate::lock::lock_workspace;
use crate::podman::{Podman, PodmanContainerInspect, PodmanContainerMount};
use crate::preflight::{check_host_prerequisites, direnv_applies_to_target};
use crate::process::ProcessRunner;
use crate::runtime::RuntimeCreateSpec;
use crate::runtime::opencode::{DEFAULT_IMAGE, OpencodeRuntime, RUNTIME_NAME};
use crate::session::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, REQUIRED_LABEL_NAMES, REQUIRED_NIX_CACHE_MOUNT_DESTINATION, SessionFailure,
    SessionRecord, SessionStatus, discover_sessions_for_git_root,
};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};
use crate::{Error, Result};

pub fn run(args: RunArgs) -> Result<()> {
    let workspace = resolve_workspace_identity(&args.directory)?;
    let mut workspace_lock = lock_workspace(&workspace)?;
    let workspace_guard = workspace_lock.guard()?;

    let preflight = check_host_prerequisites(
        Some(workspace.canonical_target.as_ref()),
        Some(workspace.canonical_git_root.as_ref()),
    )?;

    let podman = Podman::new();
    let sessions = discover_sessions_for_git_root(&podman, workspace.canonical_git_root.as_ref())?;
    match sessions.as_slice() {
        [] => {}
        [session] => {
            return Err(existing_session_error(&podman, &workspace, session));
        }
        _ => {
            return Err(duplicate_sessions_error(&workspace));
        }
    }

    let runtime = OpencodeRuntime::new();
    let process_runner = ProcessRunner::new();
    ensure_default_runtime_image(&process_runner, &runtime, &workspace, args.image.as_deref())?;
    let mut run_spec = runtime.create_spec(&workspace, args.image.as_deref(), &preflight);
    let foreground_run = foreground_run_spec(
        &runtime,
        workspace.canonical_target.as_ref(),
        workspace.canonical_git_root.as_ref(),
    );
    run_spec.command = foreground_run.argv;

    std::hint::black_box(&workspace_guard);
    drop(workspace_guard);
    drop(workspace_lock);

    podman_run_interactive(
        &process_runner,
        &workspace.container_name,
        &run_spec,
        foreground_run.workdir.as_deref(),
    )
    .map_err(|error| classify_run_error(&podman, &workspace, &run_spec, error))
}

fn ensure_default_runtime_image(
    process_runner: &ProcessRunner,
    runtime: &OpencodeRuntime,
    workspace: &WorkspaceIdentity,
    image_override: Option<&str>,
) -> Result<()> {
    if image_override.is_some() {
        return Ok(());
    }

    let podman = Podman::with_runner(process_runner.clone());
    if podman.image_exists(DEFAULT_IMAGE)? {
        return Ok(());
    }

    let context_dir = runtime.default_image_context_dir()?;
    let containerfile = context_dir.join("Containerfile");
    podman
        .build_image(DEFAULT_IMAGE, containerfile.as_ref(), context_dir.as_ref())
        .map_err(|error| {
            Error::msg(format!(
                "failed to build default runtime image `{DEFAULT_IMAGE}` for `{}` from `{}`: {error}",
                workspace.canonical_git_root, context_dir,
            ))
        })
}

fn existing_session_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    session: &SessionRecord,
) -> Error {
    if session.status == SessionStatus::Duplicate {
        return duplicate_sessions_error(workspace);
    }

    match session.status {
        SessionStatus::Running => session
            .runtime
            .as_deref()
            .filter(|runtime| *runtime != RUNTIME_NAME)
            .map(|runtime| runtime_mismatch_error(workspace, &session.container_name, runtime))
            .unwrap_or_else(|| running_existing_session_error(workspace, session)),
        SessionStatus::Stopped => session
            .runtime
            .as_deref()
            .filter(|runtime| *runtime != RUNTIME_NAME)
            .map(|runtime| runtime_mismatch_error(workspace, &session.container_name, runtime))
            .unwrap_or_else(|| stopped_existing_session_error(workspace, session)),
        SessionStatus::Orphaned => Error::msg(format!(
            "managed session `{}` for `{}` is orphaned after the repository moved; remove or recreate it before retrying",
            session.container_name, workspace.canonical_git_root,
        )),
        SessionStatus::Failed => failed_session_error(workspace, session).unwrap_or_else(|| {
            podman
                .inspect_one(&session.container_name)
                .ok()
                .and_then(|inspect| {
                    classify_named_container_conflict(workspace, &session.container_name, &inspect)
                })
                .unwrap_or_else(|| generic_failed_session_error(workspace, &session.container_name))
        }),
        SessionStatus::Duplicate => duplicate_sessions_error(workspace),
    }
}

fn failed_session_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Option<Error> {
    let failure = session.failure?;
    Some(match failure {
        SessionFailure::MissingRequiredLabels => Error::managed_session_requires_action(
            workspace.canonical_git_root.as_ref(),
            &session.container_name,
            "is missing required session labels",
            "repair or recreate it before retrying",
        ),
        SessionFailure::DriftedGitRootHash => Error::managed_session_requires_action(
            workspace.canonical_git_root.as_ref(),
            &session.container_name,
            "has a drifted `io.agentbox.git_root_hash`",
            "repair or recreate it before retrying",
        ),
        SessionFailure::MissingCacheMount => Error::managed_session_requires_action(
            workspace.canonical_git_root.as_ref(),
            &session.container_name,
            &format!(
                "is missing required cache mount `{}`",
                REQUIRED_NIX_CACHE_MOUNT_DESTINATION
            ),
            "recreate the container before retrying",
        ),
    })
}

fn classify_create_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    create_spec: &RuntimeCreateSpec,
    original_error: Error,
) -> Error {
    podman
        .inspect_one(&workspace.container_name)
        .ok()
        .and_then(|inspect| {
            classify_named_container_conflict(
                workspace,
                &create_spec.labels[LABEL_LOGICAL_NAME],
                &inspect,
            )
        })
        .unwrap_or(original_error)
}

fn classify_named_container_conflict(
    workspace: &WorkspaceIdentity,
    expected_name: &str,
    inspect: &PodmanContainerInspect,
) -> Option<Error> {
    let labels = &inspect.config.labels;
    let container_name = inspect_container_name(inspect, expected_name);
    let managed = required_label_value(labels, LABEL_MANAGED);
    let canonical_git_root = required_label_value(labels, LABEL_GIT_ROOT);
    let git_root_hash = required_label_value(labels, LABEL_GIT_ROOT_HASH);
    let runtime = required_label_value(labels, LABEL_RUNTIME);

    if managed == Some(LABEL_MANAGED_VALUE) {
        if missing_required_label(labels) {
            return Some(Error::msg(format!(
                "managed session `{}` for `{}` is missing required session labels; repair or recreate it before retrying",
                container_name, workspace.canonical_git_root,
            )));
        }

        if git_root_hash == Some(workspace.hash12.as_str())
            && canonical_git_root.is_some_and(|root| root != workspace.canonical_git_root.as_str())
        {
            return Some(Error::msg(format!(
                "managed container `{}` collides on git-root hash `{}`: stored root `{}` does not match `{}`; remove or recreate the conflicting container before retrying",
                container_name,
                workspace.hash12,
                canonical_git_root.unwrap_or("<missing>"),
                workspace.canonical_git_root,
            )));
        }

        if canonical_git_root == Some(workspace.canonical_git_root.as_str()) {
            if runtime.is_some_and(|runtime| runtime != RUNTIME_NAME) {
                return Some(runtime_mismatch_error(
                    workspace,
                    &container_name,
                    runtime.unwrap_or("unknown"),
                ));
            }

            if git_root_hash != Some(workspace.hash12.as_str()) {
                return Some(Error::msg(format!(
                    "managed session `{}` for `{}` has a drifted `io.agentbox.git_root_hash`; repair or recreate it before retrying",
                    container_name, workspace.canonical_git_root,
                )));
            }

            if !has_required_mount(&inspect.mounts, REQUIRED_NIX_CACHE_MOUNT_DESTINATION) {
                return Some(Error::msg(format!(
                    "managed session `{}` for `{}` is missing required cache mount `{}`; recreate the container before retrying",
                    container_name,
                    workspace.canonical_git_root,
                    REQUIRED_NIX_CACHE_MOUNT_DESTINATION,
                )));
            }

            return Some(generic_failed_session_error(workspace, &container_name));
        }

        if let Some(root) = canonical_git_root {
            return Some(Error::msg(format!(
                "container name `{}` is already used by managed session `{}` for `{}`; remove or rename the conflicting container before retrying `{}`",
                workspace.container_name, container_name, root, workspace.canonical_git_root,
            )));
        }
    }

    Some(Error::msg(format!(
        "container name `{}` is already in use by a different container; remove or rename that container before retrying `{}`",
        workspace.container_name, workspace.canonical_git_root,
    )))
}

fn duplicate_sessions_error(workspace: &WorkspaceIdentity) -> Error {
    Error::msg(format!(
        "duplicate managed sessions exist for `{}`; remove extras before retrying",
        workspace.canonical_git_root
    ))
}

fn running_existing_session_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Error {
    Error::msg(format!(
        "managed session `{}` is already running for `{}`; use `agentbox attach {}` to join it or `agentbox stop {}` to stop it first",
        session.container_name,
        workspace.canonical_git_root,
        workspace.requested_target,
        workspace.requested_target,
    ))
}

fn stopped_existing_session_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Error {
    Error::msg(format!(
        "managed session `{}` already exists for `{}` but is not running; use `agentbox stop {}` before retrying `agentbox run {}`",
        session.container_name,
        workspace.canonical_git_root,
        workspace.requested_target,
        workspace.requested_target,
    ))
}

fn runtime_mismatch_error(
    workspace: &WorkspaceIdentity,
    container_name: &str,
    actual_runtime: &str,
) -> Error {
    Error::msg(format!(
        "managed session `{}` for `{}` uses runtime `{}` instead of `{}`; recreate it before retrying",
        container_name, workspace.canonical_git_root, actual_runtime, RUNTIME_NAME,
    ))
}

fn generic_failed_session_error(workspace: &WorkspaceIdentity, container_name: &str) -> Error {
    Error::msg(format!(
        "managed session `{}` for `{}` is in a failed state; repair or recreate it before retrying",
        container_name, workspace.canonical_git_root,
    ))
}

fn inspect_container_name(inspect: &PodmanContainerInspect, fallback: &str) -> String {
    required_label_value(&inspect.config.labels, LABEL_LOGICAL_NAME)
        .unwrap_or(fallback)
        .to_string()
}

fn required_label_value<'a>(labels: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    labels
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn missing_required_label(labels: &BTreeMap<String, String>) -> bool {
    REQUIRED_LABEL_NAMES
        .iter()
        .any(|name| required_label_value(labels, name).is_none())
}

fn has_required_mount(mounts: &[PodmanContainerMount], destination: &str) -> bool {
    mounts.iter().any(|mount| mount.destination == destination)
}

pub(crate) struct RuntimeCommandSpec {
    pub(crate) argv: Vec<String>,
    pub(crate) workdir: Option<String>,
}

pub(crate) fn foreground_run_spec(
    runtime: &OpencodeRuntime,
    target: &Utf8Path,
    git_root: &Utf8Path,
) -> RuntimeCommandSpec {
    let base = runtime.foreground_command();
    let workdir = Some(target.to_string());

    if direnv_applies_to_target(target, git_root) {
        let mut argv = vec!["direnv".to_string(), "exec".to_string(), ".".to_string()];
        argv.extend(base.argv);
        RuntimeCommandSpec { argv, workdir }
    } else {
        RuntimeCommandSpec {
            argv: base.argv,
            workdir,
        }
    }
}

fn classify_run_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    create_spec: &RuntimeCreateSpec,
    original_error: Error,
) -> Error {
    let wrapped = Error::runtime_command_failed(
        workspace.canonical_git_root.as_ref(),
        &workspace.container_name,
        "run the foreground runtime command",
        &original_error.to_string(),
    );
    classify_create_error(podman, workspace, create_spec, wrapped)
}

fn podman_run_interactive(
    process_runner: &ProcessRunner,
    container_name: &str,
    spec: &RuntimeCreateSpec,
    workdir: Option<&str>,
) -> Result<()> {
    let mut command = process_runner.command("podman")?;
    command.arg("run");
    command.arg("--rm");
    command.args(["--name", container_name]);
    command.arg("--interactive");
    if should_allocate_tty() {
        command.arg("--tty");
    }
    if let Some(workdir) = workdir {
        command.args(["--workdir", workdir]);
    }

    for (name, value) in &spec.labels {
        command.arg("--label");
        command.arg(format!("{name}={value}"));
    }

    for mount in &spec.mounts {
        command.arg("--mount");
        command.arg(render_mount(mount));
    }

    for (name, value) in &spec.default_env {
        command.arg("--env");
        command.arg(format!("{name}={value}"));
    }

    if !spec.network_enabled {
        command.arg("--network=none");
    }

    for port in &spec.published_ports {
        command.arg("--publish");
        command.arg(port);
    }

    command.arg(&spec.image);
    command.args(&spec.command);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    let description = describe_command(&command);
    let status = command
        .status()
        .map_err(|error| Error::msg(format!("failed to run `{description}`: {error}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "`{description}` exited with {}",
            status
                .code()
                .map(|code| format!("exit status {code}"))
                .unwrap_or_else(|| "signal".to_string())
        )))
    }
}

fn should_allocate_tty() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
}

fn render_mount(mount: &crate::runtime::RuntimeMount) -> String {
    let kind = match mount.kind {
        crate::runtime::RuntimeMountKind::Bind => "bind",
        crate::runtime::RuntimeMountKind::Volume => "volume",
    };
    let mut options = vec![
        format!("type={kind}"),
        format!("src={}", mount.source),
        format!("dst={}", mount.destination),
    ];
    if mount.read_only {
        options.push("ro".to_string());
    }
    options.join(",")
}

fn describe_command(command: &Command) -> String {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
