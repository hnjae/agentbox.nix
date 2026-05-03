// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::{Utf8Path, Utf8PathBuf};

use crate::Error;
use crate::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_IMAGE,
    LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE, LABEL_RUNTIME, LABEL_SCHEMA,
    LABEL_SCHEMA_VALUE,
};
use crate::runtime::{RuntimeAttachSpec, RuntimeKind};
use crate::workspace::hash12;

use super::record::SessionMetadata;
use super::status::SessionFailure;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequiredSessionLabels {
    canonical_git_root: Utf8PathBuf,
    git_root_hash: String,
}

#[derive(Debug)]
pub(super) enum AttachLabelError {
    MissingLabel(&'static str),
    Runtime(Error),
    MalformedContainerPort(String),
    AttachSchemeMismatch {
        runtime: RuntimeKind,
        actual: String,
        expected: &'static str,
    },
    ContainerPortMismatch {
        runtime: RuntimeKind,
        actual: u16,
        expected: u16,
    },
    ContainerListenIpMismatch {
        runtime: RuntimeKind,
        actual: String,
        expected: &'static str,
    },
}

impl AttachLabelError {
    pub(super) fn session_failure(&self) -> SessionFailure {
        match self {
            Self::MissingLabel(_) => SessionFailure::MissingRequiredLabels,
            Self::Runtime(_) => SessionFailure::UnsupportedRuntimeLabel,
            Self::MalformedContainerPort(_)
            | Self::AttachSchemeMismatch { .. }
            | Self::ContainerPortMismatch { .. }
            | Self::ContainerListenIpMismatch { .. } => SessionFailure::MalformedEndpointLabels,
        }
    }

    pub(super) fn into_error(self) -> Error {
        match self {
            Self::MissingLabel(label) => Error::msg(format!("missing required label `{label}`")),
            Self::Runtime(error) => error,
            Self::MalformedContainerPort(error) => Error::msg(format!(
                "malformed `io.agentbox.container_port` label: {error}"
            )),
            Self::AttachSchemeMismatch {
                runtime,
                actual,
                expected,
            } => Error::msg(format!(
                "managed session has attach scheme `{actual}` but runtime `{runtime}` requires `{expected}`"
            )),
            Self::ContainerPortMismatch {
                runtime,
                actual,
                expected,
            } => Error::msg(format!(
                "managed session publishes container port `{actual}` but runtime `{runtime}` requires `{expected}`"
            )),
            Self::ContainerListenIpMismatch {
                runtime,
                actual,
                expected,
            } => Error::msg(format!(
                "managed session has container listen IP `{actual}` but runtime `{runtime}` requires `{expected}`"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AttachLabels {
    runtime: RuntimeKind,
    attach: RuntimeAttachSpec,
}

#[derive(Debug)]
pub(super) struct SessionLabelReport {
    required: std::result::Result<RequiredSessionLabels, SessionFailure>,
    attach: std::result::Result<AttachLabels, AttachLabelError>,
}

impl SessionLabelReport {
    pub(super) fn from_metadata(metadata: &SessionMetadata) -> Self {
        Self {
            required: RequiredSessionLabels::validated(metadata),
            attach: AttachLabels::from_session_labels(metadata),
        }
    }

    pub(super) fn required_failure(&self) -> Option<SessionFailure> {
        self.required.as_ref().err().copied()
    }

    pub(super) fn attach_failure(&self) -> Option<SessionFailure> {
        self.attach
            .as_ref()
            .err()
            .map(AttachLabelError::session_failure)
    }

    pub(super) fn canonical_git_root(&self) -> Option<&Utf8Path> {
        self.required
            .as_ref()
            .ok()
            .map(RequiredSessionLabels::canonical_git_root)
    }

    pub(super) fn attach_labels(&self) -> Option<AttachLabels> {
        self.attach.as_ref().ok().copied()
    }

    pub(super) fn runtime_kind(&self) -> Option<RuntimeKind> {
        self.attach_labels().map(AttachLabels::runtime)
    }
}

impl SessionMetadata {
    pub(super) fn attach_labels(&self) -> std::result::Result<AttachLabels, AttachLabelError> {
        AttachLabels::from_session_labels(self)
    }
}

impl RequiredSessionLabels {
    fn validated(labels: &SessionMetadata) -> std::result::Result<Self, SessionFailure> {
        let required = Self::from_session_labels(labels)?;
        if required.hash_matches_root() {
            Ok(required)
        } else {
            Err(SessionFailure::DriftedGitRootHash)
        }
    }

    fn canonical_git_root(&self) -> &Utf8Path {
        &self.canonical_git_root
    }

    fn from_session_labels(labels: &SessionMetadata) -> std::result::Result<Self, SessionFailure> {
        if labels.label(LABEL_MANAGED) != Some(LABEL_MANAGED_VALUE)
            || labels.label(LABEL_SCHEMA) != Some(LABEL_SCHEMA_VALUE)
            || labels.label(LABEL_IMAGE).is_none()
            || labels.label(LABEL_LOGICAL_NAME).is_none()
        {
            return Err(SessionFailure::MissingRequiredLabels);
        }

        let Some(canonical_git_root) = labels.canonical_git_root().map(Utf8Path::to_path_buf)
        else {
            return Err(SessionFailure::MissingRequiredLabels);
        };
        let Some(git_root_hash) = labels.git_root_hash().map(str::to_string) else {
            return Err(SessionFailure::MissingRequiredLabels);
        };
        Ok(Self {
            canonical_git_root,
            git_root_hash,
        })
    }

    fn hash_matches_root(&self) -> bool {
        self.git_root_hash == hash12(self.canonical_git_root.as_str().as_bytes())
    }
}

impl AttachLabels {
    fn from_session_labels(
        labels: &SessionMetadata,
    ) -> std::result::Result<Self, AttachLabelError> {
        let runtime = labels
            .runtime()
            .ok_or(AttachLabelError::MissingLabel(LABEL_RUNTIME))?
            .parse::<RuntimeKind>()
            .map_err(AttachLabelError::Runtime)?;
        let attach = runtime.adapter().attach_spec();

        let attach_scheme = labels
            .label(LABEL_ATTACH_SCHEME)
            .ok_or(AttachLabelError::MissingLabel(LABEL_ATTACH_SCHEME))?;
        if attach_scheme != attach.scheme {
            return Err(AttachLabelError::AttachSchemeMismatch {
                runtime,
                actual: attach_scheme.to_string(),
                expected: attach.scheme,
            });
        }

        let container_port = labels
            .label(LABEL_CONTAINER_PORT)
            .ok_or(AttachLabelError::MissingLabel(LABEL_CONTAINER_PORT))?
            .parse::<u16>()
            .map_err(|error| AttachLabelError::MalformedContainerPort(error.to_string()))?;
        if container_port != attach.container_port {
            return Err(AttachLabelError::ContainerPortMismatch {
                runtime,
                actual: container_port,
                expected: attach.container_port,
            });
        }

        let container_listen_ip = labels
            .label(LABEL_CONTAINER_LISTEN_IP)
            .ok_or(AttachLabelError::MissingLabel(LABEL_CONTAINER_LISTEN_IP))?;
        if container_listen_ip != attach.container_listen_ip {
            return Err(AttachLabelError::ContainerListenIpMismatch {
                runtime,
                actual: container_listen_ip.to_string(),
                expected: attach.container_listen_ip,
            });
        }

        Ok(Self { runtime, attach })
    }

    pub(super) fn runtime(self) -> RuntimeKind {
        self.runtime
    }

    pub(super) fn scheme(self) -> &'static str {
        self.attach.scheme
    }

    pub(super) fn container_port(self) -> u16 {
        self.attach.container_port
    }
}
