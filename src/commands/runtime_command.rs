use std::process::Stdio;

use camino::{Utf8Path, Utf8PathBuf};

use crate::direnv::wrap_exec_if_envrc_applies;
use crate::process::{ProcessRunner, format_status, run_command_status};
use crate::runtime::{AttachEndpoint, RuntimeKind};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeInvocation {
    pub(crate) argv: Vec<String>,
    pub(crate) workdir: Utf8PathBuf,
}

pub(crate) fn server_runtime_command(
    runtime: RuntimeKind,
    target: &Utf8Path,
    git_root: &Utf8Path,
) -> RuntimeInvocation {
    RuntimeInvocation {
        argv: wrap_exec_if_envrc_applies(runtime.server_command().argv, target, git_root),
        workdir: target.to_path_buf(),
    }
}

pub(crate) fn host_client_runtime_command(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
) -> RuntimeInvocation {
    RuntimeInvocation {
        argv: runtime.host_client_command(endpoint).argv,
        workdir: launch_directory.to_path_buf(),
    }
}

pub(crate) fn run_host_client(
    process_runner: &ProcessRunner,
    client: &RuntimeInvocation,
) -> Result<()> {
    let argv = &client.argv;
    let Some((program, args)) = argv.split_first() else {
        return Err(Error::msg("runtime host client command is empty"));
    };

    let mut command = process_runner.command(program)?;
    command.args(args);
    command.current_dir(client.workdir.as_std_path());
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
