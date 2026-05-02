use crate::Error;
use crate::runtime::RuntimeKind;
use crate::session::SessionRecord;
use crate::workspace::WorkspaceIdentity;

pub(crate) enum SingleSession<'a> {
    Missing,
    Found(&'a SessionRecord),
    Duplicate,
}

pub(crate) fn select_single_session(sessions: &[SessionRecord]) -> SingleSession<'_> {
    match sessions {
        [] => SingleSession::Missing,
        [session] => SingleSession::Found(session),
        _ => SingleSession::Duplicate,
    }
}

pub(crate) fn duplicate_sessions_error(workspace: &WorkspaceIdentity) -> Error {
    Error::duplicate_managed_sessions(workspace.canonical_git_root.as_ref())
}

pub(crate) fn run_command_hint(runtime: Option<&str>, workspace: &WorkspaceIdentity) -> String {
    let runtime = runtime
        .filter(|runtime| runtime.parse::<RuntimeKind>().is_ok())
        .unwrap_or("<opencode|codex>");
    format!(
        "agentbox run --runtime {runtime} {}",
        workspace.requested_target
    )
}
