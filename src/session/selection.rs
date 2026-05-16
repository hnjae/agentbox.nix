use std::collections::BTreeMap;

use crate::runtime::RuntimeKind;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::conflict::duplicate_sessions_error;
use super::record::SessionRecord;

#[derive(Debug)]
pub(crate) struct StableIdPrefixSelection<'a> {
    id: String,
    sessions: Vec<&'a SessionRecord>,
}

impl<'a> StableIdPrefixSelection<'a> {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn has_duplicate_sessions(&self) -> bool {
        self.sessions.len() > 1
    }

    pub(crate) fn into_single_session(self) -> Option<&'a SessionRecord> {
        if self.sessions.len() == 1 {
            self.sessions.into_iter().next()
        } else {
            None
        }
    }

    pub(crate) fn into_sessions(self) -> Vec<&'a SessionRecord> {
        self.sessions
    }
}

pub(crate) fn select_stable_id_prefix<'a>(
    sessions: &'a [SessionRecord],
    prefix: &str,
) -> Result<StableIdPrefixSelection<'a>> {
    select_stable_id_prefix_with_noun(sessions, prefix, "managed session")
}

pub(crate) fn select_agentbox_stable_id_prefix<'a>(
    sessions: &'a [SessionRecord],
    prefix: &str,
) -> Result<StableIdPrefixSelection<'a>> {
    select_stable_id_prefix_with_noun(sessions, prefix, "agentbox container")
}

fn select_stable_id_prefix_with_noun<'a>(
    sessions: &'a [SessionRecord],
    prefix: &str,
    noun: &str,
) -> Result<StableIdPrefixSelection<'a>> {
    if prefix.is_empty() {
        return Err(Error::msg("stable id prefix must not be empty"));
    }

    let normalized_prefix = prefix.to_ascii_lowercase();
    let mut matches = BTreeMap::<String, Vec<&SessionRecord>>::new();

    for session in sessions {
        let Some(id) = session.stable_id() else {
            continue;
        };

        if id.to_ascii_lowercase().starts_with(&normalized_prefix) {
            matches.entry(id.to_string()).or_default().push(session);
        }
    }

    match matches.len() {
        0 => Err(Error::msg(format!(
            "no {noun} id matches prefix `{prefix}`"
        ))),
        1 => {
            let (id, sessions) = matches.into_iter().next().unwrap();
            Ok(StableIdPrefixSelection { id, sessions })
        }
        _ => {
            let ids = matches.keys().cloned().collect::<Vec<_>>().join(", ");
            Err(Error::msg(format!(
                "stable id prefix `{prefix}` matches multiple ids ({ids}); use a longer prefix"
            )))
        }
    }
}

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
        "agentbox start --runtime {runtime} {}",
        workspace.requested_target
    )
}
