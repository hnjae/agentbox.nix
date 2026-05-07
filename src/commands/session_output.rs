// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use serde::Serialize;

use crate::runtime::AttachEndpoint;
use crate::session::SessionRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionDisplay<'a> {
    id: Option<&'a str>,
    canonical_git_root: Option<&'a camino::Utf8Path>,
    runtime: Option<&'a str>,
    endpoint: Option<&'a AttachEndpoint>,
    container_name: &'a str,
}

impl<'a> SessionDisplay<'a> {
    pub(super) fn from_session(session: &'a SessionRecord) -> Self {
        Self {
            id: session.stable_id(),
            canonical_git_root: session.canonical_git_root(),
            runtime: session.runtime(),
            endpoint: session.attach_endpoint.as_ref(),
            container_name: &session.container_name,
        }
    }

    pub(super) fn id(&self) -> Option<&'a str> {
        self.id
    }

    pub(super) fn id_or_unknown(&self) -> &'a str {
        self.id.unwrap_or("unknown")
    }

    pub(super) fn canonical_git_root_str(&self) -> Option<&'a str> {
        self.canonical_git_root.map(camino::Utf8Path::as_str)
    }

    pub(super) fn canonical_git_root_or_unknown(&self) -> &'a str {
        self.canonical_git_root_str().unwrap_or("unknown")
    }

    pub(super) fn runtime(&self) -> Option<&'a str> {
        self.runtime
    }

    pub(super) fn runtime_or_unknown(&self) -> &'a str {
        self.runtime.unwrap_or("unknown")
    }

    pub(super) fn endpoint_string(&self) -> Option<String> {
        self.endpoint.map(ToString::to_string)
    }

    pub(super) fn endpoint_or_unknown(&self) -> String {
        self.endpoint_string()
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub(super) fn container_name(&self) -> &'a str {
        self.container_name
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionJsonFields<'a> {
    pub(super) leading: SessionJsonLeadingFields<'a>,
    pub(super) trailing: SessionJsonTrailingFields<'a>,
}

impl<'a> SessionJsonFields<'a> {
    pub(super) fn from_session(session: &'a SessionRecord) -> Self {
        Self::from_display(&SessionDisplay::from_session(session))
    }

    pub(super) fn from_display(display: &SessionDisplay<'a>) -> Self {
        Self {
            leading: SessionJsonLeadingFields {
                id: display.id(),
                canonical_git_root: display.canonical_git_root_str(),
                runtime: display.runtime(),
            },
            trailing: SessionJsonTrailingFields {
                endpoint: display.endpoint_string(),
                container_name: display.container_name(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct SessionJsonLeadingFields<'a> {
    id: Option<&'a str>,
    canonical_git_root: Option<&'a str>,
    runtime: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct SessionJsonTrailingFields<'a> {
    endpoint: Option<String>,
    container_name: &'a str,
}
