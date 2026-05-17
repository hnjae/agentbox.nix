// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod collection;
mod conflict;
mod connectable;
mod discovery;
mod endpoint;
mod labels;
mod mounts;
mod record;
mod restartable;
mod selection;
mod status;
mod target;
mod target_candidate;

pub use collection::group_sessions_by_git_root;
pub(crate) use collection::{
    exact_git_root_matches, partition_sessions_by_git_root, sort_session_refs_by_identity,
    sorted_session_refs_by_identity,
};
pub(crate) use conflict::duplicate_agentbox_containers_error;
pub(crate) use conflict::{classify_create_error_or_else, existing_session_error};
pub(crate) use connectable::prepare_connect_session;
pub use discovery::SessionDiscoveryQuery;
pub use endpoint::discover_attach_endpoint_from_inspect;
pub use record::{SessionGroup, SessionMetadata, SessionRecord};
pub(crate) use restartable::prepare_restart_session;
pub(crate) use selection::{run_command_hint, select_single_session, select_stable_id_prefix};
pub(crate) use target::{
    RestartSessionTargetPlan, SessionTargetInput, StopExactGitRootTarget, StopSessionTargetPlan,
    StopStableIdTarget,
};
pub(crate) use target_candidate::{SessionTargetCandidate, SessionTargetKind};

pub use status::{
    SessionFailure, SessionStatus, failed_session_requires_action_error,
    resource_failure_requires_action_error, session_failure_requires_action_error,
};

pub use crate::preflight::NIX_CACHE_DESTINATION as REQUIRED_NIX_CACHE_MOUNT_DESTINATION;
