use crate::Result;
use crate::runtime::RuntimeKind;
use crate::workspace::WorkspaceIdentity;

use super::conflict::duplicate_sessions_error;
use super::record::SessionRecord;

pub(crate) fn select_single_session<'a>(
    sessions: &'a [SessionRecord],
    workspace: &WorkspaceIdentity,
) -> Result<Option<&'a SessionRecord>> {
    match sessions {
        [] => Ok(None),
        [session] => Ok(Some(session)),
        _ => Err(duplicate_sessions_error(workspace)),
    }
}

pub(crate) fn run_command_hint(runtime: Option<&str>, workspace: &WorkspaceIdentity) -> String {
    let runtime = runtime
        .filter(|runtime| runtime.parse::<RuntimeKind>().is_ok())
        .map(str::to_string)
        .unwrap_or_else(RuntimeKind::supported_values_placeholder);
    format!(
        "agentbox run --runtime {runtime} {}",
        workspace.requested_target
    )
}
