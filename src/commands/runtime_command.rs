// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::process::{ExitStatus, Stdio};

use camino::Utf8Path;

use crate::dev_env::DevEnvironment;
use crate::process::{ProcessRunner, format_status, run_command_status};
use crate::runtime::{AttachEndpoint, DEFAULT_HOST_ATTACH_IP, RuntimeInvocation, RuntimeKind};
use crate::{Error, Result};

pub(crate) fn server_runtime_command(
    runtime: RuntimeKind,
    target: &Utf8Path,
    dev_env: &DevEnvironment,
) -> RuntimeInvocation {
    RuntimeInvocation::new(
        dev_env.wrap_argv(runtime.server_command().argv),
        target.to_path_buf(),
    )
}

pub(crate) fn codex_exec_runtime_command(
    target: &Utf8Path,
    dev_env: &DevEnvironment,
    codex_args: Vec<String>,
) -> RuntimeInvocation {
    let mut argv = RuntimeKind::Codex.foreground_command().argv;
    argv.push("exec".to_string());
    argv.extend(["--disable".to_string(), "codex_git_commit".to_string()]);
    argv.extend(codex_args);

    RuntimeInvocation::new(dev_env.wrap_argv(argv), target.to_path_buf())
}

fn host_client_runtime_command(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
) -> RuntimeInvocation {
    RuntimeInvocation::new(
        runtime.host_client_command(endpoint).argv,
        launch_directory.to_path_buf(),
    )
}

pub(crate) fn run_host_runtime_client(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
) -> Result<()> {
    let status = run_host_runtime_client_status(runtime, endpoint, launch_directory)?;
    if status.success() {
        Ok(())
    } else {
        Err(host_client_status_error(
            runtime,
            endpoint,
            launch_directory,
            status,
        ))
    }
}

pub(crate) fn run_host_runtime_client_status(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
) -> Result<ExitStatus> {
    let process_runner = ProcessRunner::new();
    let client = host_client_runtime_command(runtime, endpoint, launch_directory);

    run_host_client(&process_runner, &client)
}

pub(crate) fn ensure_host_runtime_client_available(runtime: RuntimeKind) -> Result<()> {
    let attach = runtime.attach_spec();
    let endpoint = AttachEndpoint {
        scheme: attach.scheme.to_string(),
        host_ip: DEFAULT_HOST_ATTACH_IP.to_string(),
        host_port: attach.container_port,
    };
    let command = runtime.host_client_command(&endpoint);
    let Some(program) = command.argv.first() else {
        return Err(Error::msg("runtime host client command is empty"));
    };

    which::which(program)
        .map(|_| ())
        .map_err(|_| Error::msg(host_client_not_found_message(program)))
}

pub(crate) fn host_client_status_error(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
    status: ExitStatus,
) -> Error {
    let client = host_client_runtime_command(runtime, endpoint, launch_directory);
    Error::msg(format!(
        "`{}` exited with {}",
        client.argv().join(" "),
        format_status(status)
    ))
}

fn run_host_client(
    process_runner: &ProcessRunner,
    client: &RuntimeInvocation,
) -> Result<ExitStatus> {
    let argv = client.argv();
    let Some((program, args)) = argv.split_first() else {
        return Err(Error::msg("runtime host client command is empty"));
    };

    let mut command = process_runner.command(program)?;
    command.args(args);
    command.current_dir(client.workdir().as_std_path());
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());

    run_command_status(&mut command)
}

fn host_client_not_found_message(program: &str) -> String {
    format!("`{program}` was not found on PATH; install `{program}` or add it to PATH")
}
