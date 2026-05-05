use camino::{Utf8Path, Utf8PathBuf};

use crate::direnv::wrap_exec_if_envrc_applies;
use crate::runtime::{AttachEndpoint, RuntimeKind};

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
