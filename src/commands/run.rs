// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use camino::Utf8Path;

use crate::cli::RunArgs;
use crate::lock::lock_workspace;
use crate::podman::{Podman, PodmanContainerInspect, PodmanContainerMount};
use crate::preflight::{check_host_prerequisites, direnv_applies_to_target};
use crate::process::{ProcessRunner, run_command};
use crate::runtime::RuntimeCreateSpec;
use crate::runtime::opencode::{OpencodeRuntime, RUNTIME_NAME};
use crate::session::{
    LABEL_GIT_ROOT, LABEL_GIT_ROOT_HASH, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, REQUIRED_LABEL_NAMES, REQUIRED_NIX_CACHE_MOUNT_DESTINATION, SessionRecord,
    SessionStatus, discover_sessions_for_git_root,
};
use crate::workspace::{WorkspaceIdentity, resolve_workspace_identity};
use crate::{Error, Result};

const READINESS_ATTEMPTS: usize = 30;
const READINESS_DELAY: Duration = Duration::from_millis(200);

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
    let create_spec = runtime.create_spec(&workspace, args.image.as_deref(), &preflight);
    podman_create(&process_runner, &workspace.container_name, &create_spec)
        .map_err(|error| classify_create_error(&podman, &workspace, &create_spec, error))?;
    podman_start(&process_runner, &workspace.container_name)?;

    let server_start = server_start_spec(
        &runtime,
        workspace.canonical_target.as_ref(),
        workspace.canonical_git_root.as_ref(),
    );
    podman_exec(
        &process_runner,
        &workspace.container_name,
        &server_start.argv,
        server_start.workdir.as_deref(),
        true,
    )?;

    wait_for_readiness(&process_runner, &workspace.container_name, &runtime)?;

    std::hint::black_box(&workspace_guard);
    drop(workspace_guard);
    drop(workspace_lock);

    podman_exec_interactive(
        &process_runner,
        &workspace.container_name,
        &runtime
            .attach_command(workspace.canonical_target.as_ref())
            .argv,
        None,
    )
}

fn existing_session_error(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    session: &SessionRecord,
) -> Error {
    if session.status == SessionStatus::Duplicate {
        return duplicate_sessions_error(workspace);
    }

    if session
        .runtime
        .as_deref()
        .is_some_and(|runtime| runtime != RUNTIME_NAME)
    {
        return runtime_mismatch_error(
            workspace,
            &session.container_name,
            session.runtime.as_deref().unwrap_or("unknown"),
        );
    }

    match session.status {
        SessionStatus::Running | SessionStatus::Stopped => {
            attach_existing_session_error(workspace, session)
        }
        SessionStatus::Orphaned => Error::msg(format!(
            "managed session `{}` for `{}` is orphaned after the repository moved; remove or recreate it before retrying",
            session.container_name, workspace.canonical_git_root,
        )),
        SessionStatus::Failed => podman
            .inspect_one(&session.container_name)
            .ok()
            .and_then(|inspect| {
                classify_named_container_conflict(workspace, &session.container_name, &inspect)
            })
            .unwrap_or_else(|| generic_failed_session_error(workspace, &session.container_name)),
        SessionStatus::Duplicate => duplicate_sessions_error(workspace),
    }
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

fn attach_existing_session_error(workspace: &WorkspaceIdentity, session: &SessionRecord) -> Error {
    Error::msg(format!(
        "managed session `{}` already exists for `{}`; use `agentbox attach {}` instead",
        session.container_name, workspace.canonical_git_root, workspace.requested_target
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

struct ServerStartSpec {
    argv: Vec<String>,
    workdir: Option<String>,
}

fn server_start_spec(
    runtime: &OpencodeRuntime,
    target: &Utf8Path,
    git_root: &Utf8Path,
) -> ServerStartSpec {
    let base = runtime.detached_server_start();
    let workdir = Some(target.to_string());

    if direnv_applies_to_target(target, git_root) {
        let mut argv = vec!["direnv".to_string(), "exec".to_string(), ".".to_string()];
        argv.extend(base.argv);
        ServerStartSpec { argv, workdir }
    } else {
        ServerStartSpec {
            argv: base.argv,
            workdir,
        }
    }
}

fn wait_for_readiness(
    process_runner: &ProcessRunner,
    container_name: &str,
    runtime: &OpencodeRuntime,
) -> Result<()> {
    let probe = runtime.health_probe();
    let mut last_error = None;

    for attempt in 0..READINESS_ATTEMPTS {
        match podman_exec(process_runner, container_name, &probe.argv, None, false) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }

        if attempt + 1 < READINESS_ATTEMPTS {
            thread::sleep(READINESS_DELAY);
        }
    }

    let detail = last_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "no readiness probe was executed".to_string());
    Err(Error::msg(format!(
        "runtime for `{container_name}` did not become ready: {detail}"
    )))
}

fn podman_create(
    process_runner: &ProcessRunner,
    container_name: &str,
    spec: &RuntimeCreateSpec,
) -> Result<()> {
    let mut command = process_runner.command("podman")?;
    command.arg("create");
    command.args(["--name", container_name]);

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
    run_command(&mut command).map(|_| ())
}

fn podman_start(process_runner: &ProcessRunner, container_name: &str) -> Result<()> {
    let mut command = process_runner.command("podman")?;
    command.args(["start", container_name]);
    run_command(&mut command).map(|_| ())
}

fn podman_exec(
    process_runner: &ProcessRunner,
    container_name: &str,
    argv: &[String],
    workdir: Option<&str>,
    detached: bool,
) -> Result<()> {
    let mut command = process_runner.command("podman")?;
    command.arg("exec");
    if detached {
        command.arg("--detach");
    }
    if let Some(workdir) = workdir {
        command.args(["--workdir", workdir]);
    }
    command.arg(container_name);
    command.args(argv);
    run_command(&mut command).map(|_| ())
}

fn podman_exec_interactive(
    process_runner: &ProcessRunner,
    container_name: &str,
    argv: &[String],
    workdir: Option<&str>,
) -> Result<()> {
    let mut command = process_runner.command("podman")?;
    command.arg("exec");
    if let Some(workdir) = workdir {
        command.args(["--workdir", workdir]);
    }
    command.arg(container_name);
    command.args(argv);
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
