// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

mod conflict;
mod discovery;
mod endpoint;
mod labels;
mod mounts;
mod record;
mod status;

pub(crate) use conflict::{
    classify_create_error, duplicate_sessions_error, existing_session_error,
};
pub use discovery::{
    discover_managed_sessions, discover_managed_sessions_from_ps, discover_sessions_for_git_root,
    discover_sessions_for_git_root_from_ps, group_sessions_by_git_root,
};
pub use endpoint::discover_attach_endpoint_from_inspect;
pub use record::{SessionGroup, SessionRecord};

pub use crate::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, REQUIRED_LABEL_NAMES,
};

pub(crate) use crate::metadata::{missing_required_label, required_label_value};
pub use status::{
    SessionFailure, SessionStatus, failed_session_requires_action_error,
    session_failure_requires_action_error,
};

pub const REQUIRED_NIX_CACHE_MOUNT_DESTINATION: &str = "/home/user/.cache/nix";
