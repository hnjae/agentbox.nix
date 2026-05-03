// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::{Utf8Path, Utf8PathBuf};

use std::collections::BTreeMap;

use crate::Error;
use crate::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_IMAGE, LABEL_LOGICAL_NAME, LABEL_MANAGED, LABEL_MANAGED_VALUE,
    LABEL_RUNTIME, LABEL_SCHEMA, LABEL_SCHEMA_VALUE, required_label_string, required_label_value,
};
use crate::runtime::{RuntimeAttachSpec, RuntimeKind};
use crate::workspace::hash12;

use super::record::SessionMetadata;
use super::status::SessionFailure;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ValidSessionLabels {
    required: RequiredSessionLabels,
}

impl ValidSessionLabels {
    pub(super) fn canonical_git_root(&self) -> &Utf8Path {
        &self.required.canonical_git_root
    }
}

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
    attach: RuntimeAttachSpec,
}

impl SessionMetadata {
    pub(super) fn from_map(labels: &BTreeMap<String, String>) -> Self {
        Self {
            managed: required_label_string(labels, LABEL_MANAGED),
            schema: required_label_string(labels, LABEL_SCHEMA),
            canonical_git_root: required_label_value(labels, LABEL_GIT_ROOT).map(Utf8PathBuf::from),
            git_root_hash: required_label_string(labels, LABEL_GIT_ROOT_HASH),
            runtime: required_label_string(labels, LABEL_RUNTIME),
            image: required_label_string(labels, LABEL_IMAGE),
            logical_name: required_label_string(labels, LABEL_LOGICAL_NAME),
            attach_scheme: required_label_string(labels, LABEL_ATTACH_SCHEME),
            container_port: required_label_string(labels, LABEL_CONTAINER_PORT),
            container_listen_ip: required_label_string(labels, LABEL_CONTAINER_LISTEN_IP),
        }
    }

    pub(super) fn is_managed(&self) -> bool {
        self.managed.as_deref() == Some(LABEL_MANAGED_VALUE)
    }

    pub(super) fn has_all_required_label_values(&self) -> bool {
        self.managed.is_some()
            && self.schema.is_some()
            && self.canonical_git_root.is_some()
            && self.git_root_hash.is_some()
            && self.runtime.is_some()
            && self.image.is_some()
            && self.logical_name.is_some()
            && self.attach_scheme.is_some()
            && self.container_port.is_some()
            && self.container_listen_ip.is_some()
    }

    pub(super) fn canonical_git_root(&self) -> Option<&Utf8Path> {
        self.canonical_git_root.as_deref()
    }

    pub(super) fn git_root_hash(&self) -> Option<&str> {
        self.git_root_hash.as_deref()
    }

    pub(super) fn logical_name_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.logical_name.as_deref().unwrap_or(fallback)
    }

    pub(super) fn validate(&self) -> std::result::Result<ValidSessionLabels, SessionFailure> {
        let required = RequiredSessionLabels::from_session_labels(self)?;

        if !required.hash_matches_root() {
            return Err(SessionFailure::DriftedGitRootHash);
        }

        AttachLabels::from_session_labels(self).map_err(|error| error.session_failure())?;

        Ok(ValidSessionLabels { required })
    }

    pub(super) fn attach_labels(&self) -> std::result::Result<AttachLabels, AttachLabelError> {
        AttachLabels::from_session_labels(self)
    }
}

impl RequiredSessionLabels {
    fn from_session_labels(labels: &SessionMetadata) -> std::result::Result<Self, SessionFailure> {
        if labels.managed.as_deref() != Some(LABEL_MANAGED_VALUE)
            || labels.schema.as_deref() != Some(LABEL_SCHEMA_VALUE)
            || labels.image.is_none()
            || labels.logical_name.is_none()
        {
            return Err(SessionFailure::MissingRequiredLabels);
        }

        let Some(canonical_git_root) = labels.canonical_git_root.clone() else {
            return Err(SessionFailure::MissingRequiredLabels);
        };
        let Some(git_root_hash) = labels.git_root_hash.clone() else {
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
            .runtime
            .as_deref()
            .ok_or(AttachLabelError::MissingLabel(LABEL_RUNTIME))?
            .parse::<RuntimeKind>()
            .map_err(AttachLabelError::Runtime)?;
        let attach = runtime.adapter().attach_spec();

        let attach_scheme = labels
            .attach_scheme
            .as_deref()
            .ok_or(AttachLabelError::MissingLabel(LABEL_ATTACH_SCHEME))?;
        if attach_scheme != attach.scheme {
            return Err(AttachLabelError::AttachSchemeMismatch {
                runtime,
                actual: attach_scheme.to_string(),
                expected: attach.scheme,
            });
        }

        let container_port = labels
            .container_port
            .as_deref()
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
            .container_listen_ip
            .as_deref()
            .ok_or(AttachLabelError::MissingLabel(LABEL_CONTAINER_LISTEN_IP))?;
        if container_listen_ip != attach.container_listen_ip {
            return Err(AttachLabelError::ContainerListenIpMismatch {
                runtime,
                actual: container_listen_ip.to_string(),
                expected: attach.container_listen_ip,
            });
        }

        Ok(Self { attach })
    }

    pub(super) fn scheme(self) -> &'static str {
        self.attach.scheme
    }

    pub(super) fn container_port(self) -> u16 {
        self.attach.container_port
    }
}
