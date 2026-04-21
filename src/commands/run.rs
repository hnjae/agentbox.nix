// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use camino::Utf8Path;

use crate::cli::RunArgs;
use crate::lock::lock_workspace;
use crate::podman::Podman;
use crate::preflight::{check_host_prerequisites, direnv_applies_to_target};
use crate::process::{ProcessRunner, run_command};
use crate::runtime::RuntimeCreateSpec;
use crate::runtime::opencode::OpencodeRuntime;
use crate::session::{SessionStatus, discover_sessions_for_git_root};
use crate::workspace::resolve_workspace_identity;
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

    let sessions =
        discover_sessions_for_git_root(&Podman::new(), workspace.canonical_git_root.as_ref())?;
    match sessions.as_slice() {
        [] => {}
        [session] => {
            return match session.status {
                SessionStatus::Duplicate => Err(Error::msg(format!(
                    "duplicate managed sessions exist for `{}`; remove extras before retrying",
                    workspace.canonical_git_root
                ))),
                _ => Err(Error::msg(format!(
                    "managed session `{}` already exists for `{}`; use `agentbox attach {}` instead",
                    session.container_name,
                    workspace.canonical_git_root,
                    workspace.requested_target
                ))),
            };
        }
        _ => {
            return Err(Error::msg(format!(
                "duplicate managed sessions exist for `{}`; remove extras before retrying",
                workspace.canonical_git_root
            )));
        }
    }

    let runtime = OpencodeRuntime::new();
    let process_runner = ProcessRunner::new();
    let create_spec = runtime.create_spec(&workspace, args.image.as_deref(), &preflight);
    podman_create(&process_runner, &workspace.container_name, &create_spec)?;
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
    if direnv_applies_to_target(target, git_root) {
        let mut argv = vec!["direnv".to_string(), "exec".to_string(), ".".to_string()];
        argv.extend(base.argv);
        ServerStartSpec {
            argv,
            workdir: Some(target.to_string()),
        }
    } else {
        ServerStartSpec {
            argv: base.argv,
            workdir: None,
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
