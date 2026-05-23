// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::process::{ExitStatus, Stdio};

use camino::Utf8Path;

use crate::dev_env::DevEnvironment;
use crate::process::{ProcessRunner, format_status};
use crate::runtime::{
    AttachEndpoint, CODEX_REMOTE_TOKEN_ENV, DEFAULT_HOST_ATTACH_IP, RuntimeInvocation, RuntimeKind,
};
use crate::{Error, Result};

use super::codex_attach_auth::CodexAttachToken;

pub(crate) fn server_runtime_command(
    runtime: RuntimeKind,
    target: &Utf8Path,
    dev_env: &DevEnvironment,
    codex_attach_token: Option<&CodexAttachToken>,
) -> Result<RuntimeInvocation> {
    let mut argv = runtime.server_command().argv;
    if runtime == RuntimeKind::Codex {
        let token = codex_attach_token
            .ok_or_else(|| Error::msg("missing Codex attach token for runtime server command"))?;
        argv.extend([
            "--ws-auth".to_string(),
            "capability-token".to_string(),
            "--ws-token-sha256".to_string(),
            token.sha256().to_string(),
        ]);
    }

    Ok(RuntimeInvocation::new(
        dev_env.wrap_argv(argv),
        target.to_path_buf(),
    ))
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
    codex_attach_token: Option<&CodexAttachToken>,
) -> Result<()> {
    let status =
        run_host_runtime_client_status(runtime, endpoint, launch_directory, codex_attach_token)?;
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
    codex_attach_token: Option<&CodexAttachToken>,
) -> Result<ExitStatus> {
    let process_runner = ProcessRunner::new();
    let client = host_client_runtime_command(runtime, endpoint, launch_directory);
    let codex_attach_token = required_codex_client_token(runtime, codex_attach_token)?;

    run_host_client(&process_runner, &client, codex_attach_token)
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
    codex_attach_token: Option<&CodexAttachToken>,
) -> Result<ExitStatus> {
    let argv = client.argv();
    let Some((program, args)) = argv.split_first() else {
        return Err(Error::msg("runtime host client command is empty"));
    };

    process_runner
        .configured_command(program, |command| {
            command.args(args);
            command.current_dir(client.workdir().as_std_path());
            if let Some(token) = codex_attach_token {
                command.env(CODEX_REMOTE_TOKEN_ENV, token.value());
            }
            command.stdin(Stdio::inherit());
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());
        })?
        .status()
}

fn required_codex_client_token(
    runtime: RuntimeKind,
    token: Option<&CodexAttachToken>,
) -> Result<Option<&CodexAttachToken>> {
    if runtime == RuntimeKind::Codex {
        token
            .map(Some)
            .ok_or_else(|| Error::msg("missing Codex attach token for host client command"))
    } else {
        Ok(None)
    }
}

fn host_client_not_found_message(program: &str) -> String {
    format!("`{program}` was not found on PATH; install `{program}` or add it to PATH")
}
