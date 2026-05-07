use std::process::Stdio;

use camino::Utf8Path;

use crate::direnv::wrap_exec_if_envrc_applies;
use crate::process::{ProcessRunner, format_status, run_command_status};
use crate::runtime::{AttachEndpoint, RuntimeInvocation, RuntimeKind};
use crate::{Error, Result};

pub(crate) fn server_runtime_command(
    runtime: RuntimeKind,
    target: &Utf8Path,
    git_root: &Utf8Path,
) -> RuntimeInvocation {
    RuntimeInvocation::new(
        wrap_exec_if_envrc_applies(runtime.server_command().argv, target, git_root),
        target.to_path_buf(),
    )
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
    let process_runner = ProcessRunner::new();
    let client = host_client_runtime_command(runtime, endpoint, launch_directory);

    run_host_client(&process_runner, &client)
}

fn run_host_client(process_runner: &ProcessRunner, client: &RuntimeInvocation) -> Result<()> {
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

    let status = run_command_status(&mut command)?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "`{}` exited with {}",
            argv.join(" "),
            format_status(status)
        )))
    }
}
