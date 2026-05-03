use camino::{Utf8Path, Utf8PathBuf};

use crate::direnv::wrap_exec_if_envrc_applies;
use crate::runtime::{AttachEndpoint, RuntimeKind};
use crate::workspace::WorkspaceIdentity;

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
    runtime_invocation(runtime.server_command().argv, target, git_root)
}

pub(crate) fn host_client_runtime_command(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    workspace: &WorkspaceIdentity,
) -> RuntimeInvocation {
    runtime_invocation(
        runtime.host_client_command(endpoint).argv,
        workspace.canonical_target.as_ref(),
        workspace.canonical_git_root.as_ref(),
    )
}

fn runtime_invocation(
    base_argv: Vec<String>,
    target: &Utf8Path,
    git_root: &Utf8Path,
) -> RuntimeInvocation {
    let argv = wrap_exec_if_envrc_applies(base_argv, target, git_root);

    RuntimeInvocation {
        argv,
        workdir: target.to_path_buf(),
    }
}
