// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

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
            endpoint: session.attach_endpoint(),
            container_name: session.container_name(),
        }
    }

    fn id(&self) -> Option<&'a str> {
        self.id
    }

    fn id_or_unknown(&self) -> &'a str {
        self.id.unwrap_or("unknown")
    }

    fn canonical_git_root_str(&self) -> Option<&'a str> {
        self.canonical_git_root.map(camino::Utf8Path::as_str)
    }

    fn canonical_git_root_or_unknown(&self) -> &'a str {
        self.canonical_git_root_str().unwrap_or("unknown")
    }

    fn runtime(&self) -> Option<&'a str> {
        self.runtime
    }

    fn runtime_or_unknown(&self) -> &'a str {
        self.runtime.unwrap_or("unknown")
    }

    fn endpoint_string(&self) -> Option<String> {
        self.endpoint.map(ToString::to_string)
    }

    fn endpoint_or_unknown(&self) -> String {
        self.endpoint_string()
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn container_name(&self) -> &'a str {
        self.container_name
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionTableFields<'a> {
    pub(super) id: &'a str,
    pub(super) canonical_git_root: &'a str,
    pub(super) runtime: &'a str,
    pub(super) endpoint: String,
}

impl<'a> SessionTableFields<'a> {
    pub(super) fn from_session(session: &'a SessionRecord) -> Self {
        Self::from_display(&SessionDisplay::from_session(session))
    }

    pub(super) fn from_display(display: &SessionDisplay<'a>) -> Self {
        Self {
            id: display.id_or_unknown(),
            canonical_git_root: display.canonical_git_root_or_unknown(),
            runtime: display.runtime_or_unknown(),
            endpoint: display.endpoint_or_unknown(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionJsonFields<'a> {
    pub(super) id: SessionJsonIdField<'a>,
    pub(super) metadata: SessionJsonMetadataFields<'a>,
    pub(super) trailing: SessionJsonTrailingFields<'a>,
}

impl<'a> SessionJsonFields<'a> {
    pub(super) fn from_session(session: &'a SessionRecord) -> Self {
        Self::from_display(&SessionDisplay::from_session(session))
    }

    pub(super) fn from_display(display: &SessionDisplay<'a>) -> Self {
        Self {
            id: SessionJsonIdField { id: display.id() },
            metadata: SessionJsonMetadataFields {
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
pub(super) struct SessionJsonIdField<'a> {
    pub(super) id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct SessionJsonMetadataFields<'a> {
    pub(super) canonical_git_root: Option<&'a str>,
    pub(super) runtime: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct SessionJsonTrailingFields<'a> {
    pub(super) endpoint: Option<String>,
    pub(super) container_name: &'a str,
}
