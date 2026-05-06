// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

mod attachable;
mod collection;
mod conflict;
mod discovery;
mod endpoint;
mod labels;
mod mounts;
mod record;
mod selection;
mod status;

pub(crate) use attachable::prepare_attach_session;
pub use collection::group_sessions_by_git_root;
pub(crate) use collection::{
    exact_git_root_matches, partition_sessions_by_git_root, sort_session_refs_by_identity,
    sorted_session_refs_by_identity,
};
pub(crate) use conflict::{classify_create_error_or_else, existing_session_error};
pub use discovery::{
    discover_managed_sessions, discover_managed_sessions_from_ps, discover_sessions_for_git_root,
    discover_sessions_for_git_root_from_ps,
};
pub use endpoint::discover_attach_endpoint_from_inspect;
pub(crate) use record::SessionDisplay;
pub use record::{SessionGroup, SessionMetadata, SessionRecord};
pub(crate) use selection::{run_command_hint, select_single_session, select_stable_id_prefix};

pub use status::{
    SessionFailure, SessionStatus, failed_session_requires_action_error,
    session_failure_requires_action_error,
};

pub use crate::preflight::NIX_CACHE_DESTINATION as REQUIRED_NIX_CACHE_MOUNT_DESTINATION;
