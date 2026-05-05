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
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_GIT_ROOT,
    LABEL_GIT_ROOT_HASH, LABEL_LAUNCH_DIRECTORY, LABEL_RUNTIME, REQUIRED_SESSION_IDENTITY_LABELS,
    REQUIRED_SESSION_MARKER_LABEL_VALUES,
};
use crate::runtime::{RuntimeAttachSpec, RuntimeKind};
use crate::workspace::hash12;

use super::record::SessionMetadata;
use super::status::SessionFailure;

type RequiredLabelsResult<T> = std::result::Result<T, SessionFailure>;
type AttachLabelsResult<T> = std::result::Result<T, AttachLabelError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequiredSessionLabels {
    canonical_git_root: Utf8PathBuf,
    git_root_hash: String,
    launch_directory: Utf8PathBuf,
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
    required: RequiredLabelsResult<RequiredSessionLabels>,
    attach: AttachLabelsResult<AttachLabels>,
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

    pub(super) fn complete_required_labels(&self) -> RequiredLabelsResult<&RequiredSessionLabels> {
        let required = self.required.as_ref().map_err(|failure| *failure)?;

        if let Some(failure) = self.attach_failure() {
            return Err(failure);
        }

        Ok(required)
    }
}

impl SessionMetadata {
    pub(super) fn attach_labels(&self) -> AttachLabelsResult<AttachLabels> {
        AttachLabels::from_session_labels(self)
    }
}

impl RequiredSessionLabels {
    fn validated(labels: &SessionMetadata) -> RequiredLabelsResult<Self> {
        let required = Self::from_session_labels(labels)?;
        if !required.hash_matches_root() {
            return Err(SessionFailure::DriftedGitRootHash);
        }

        if !required.launch_directory_is_valid() {
            return Err(SessionFailure::MalformedLaunchDirectory);
        }

        Ok(required)
    }

    pub(super) fn canonical_git_root(&self) -> &Utf8Path {
        &self.canonical_git_root
    }

    pub(super) fn git_root_hash(&self) -> &str {
        &self.git_root_hash
    }

    fn from_session_labels(labels: &SessionMetadata) -> RequiredLabelsResult<Self> {
        for (name, expected) in REQUIRED_SESSION_MARKER_LABEL_VALUES {
            require_session_label_value(labels, name, expected)?;
        }

        require_session_labels(labels, REQUIRED_SESSION_IDENTITY_LABELS)?;

        let canonical_git_root = Utf8PathBuf::from(require_session_label(labels, LABEL_GIT_ROOT)?);
        let git_root_hash = require_session_label(labels, LABEL_GIT_ROOT_HASH)?.to_string();
        let launch_directory =
            Utf8PathBuf::from(require_session_label(labels, LABEL_LAUNCH_DIRECTORY)?);

        Ok(Self {
            canonical_git_root,
            git_root_hash,
            launch_directory,
        })
    }

    fn hash_matches_root(&self) -> bool {
        self.git_root_hash == hash12(self.canonical_git_root.as_str().as_bytes())
    }

    fn launch_directory_is_valid(&self) -> bool {
        self.launch_directory.is_absolute()
            && self.launch_directory.starts_with(&self.canonical_git_root)
    }
}

impl AttachLabels {
    fn from_session_labels(labels: &SessionMetadata) -> AttachLabelsResult<Self> {
        let runtime = require_runtime_kind(labels)?;
        let attach = runtime.attach_spec();

        require_matching_attach_label(labels, LABEL_ATTACH_SCHEME, attach.scheme, |actual| {
            AttachLabelError::AttachSchemeMismatch {
                runtime,
                actual,
                expected: attach.scheme,
            }
        })?;
        require_matching_container_port(labels, runtime, attach.container_port)?;
        require_matching_attach_label(
            labels,
            LABEL_CONTAINER_LISTEN_IP,
            attach.container_listen_ip,
            |actual| AttachLabelError::ContainerListenIpMismatch {
                runtime,
                actual,
                expected: attach.container_listen_ip,
            },
        )?;

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

fn require_session_label<'a>(
    labels: &'a SessionMetadata,
    name: &'static str,
) -> RequiredLabelsResult<&'a str> {
    labels
        .label(name)
        .ok_or(SessionFailure::MissingRequiredLabels)
}

fn require_session_label_value(
    labels: &SessionMetadata,
    name: &'static str,
    expected: &'static str,
) -> RequiredLabelsResult<()> {
    match labels.label(name) {
        Some(actual) if actual == expected => Ok(()),
        _ => Err(SessionFailure::MissingRequiredLabels),
    }
}

fn require_session_labels(
    labels: &SessionMetadata,
    names: &[&'static str],
) -> RequiredLabelsResult<()> {
    for name in names {
        require_session_label(labels, name)?;
    }

    Ok(())
}

fn require_attach_label<'a>(
    labels: &'a SessionMetadata,
    name: &'static str,
) -> AttachLabelsResult<&'a str> {
    labels
        .label(name)
        .ok_or(AttachLabelError::MissingLabel(name))
}

fn require_runtime_kind(labels: &SessionMetadata) -> AttachLabelsResult<RuntimeKind> {
    require_attach_label(labels, LABEL_RUNTIME)?
        .parse::<RuntimeKind>()
        .map_err(AttachLabelError::Runtime)
}

fn require_matching_attach_label(
    labels: &SessionMetadata,
    name: &'static str,
    expected: &'static str,
    mismatch: impl FnOnce(String) -> AttachLabelError,
) -> AttachLabelsResult<()> {
    let actual = require_attach_label(labels, name)?;
    if actual == expected {
        Ok(())
    } else {
        Err(mismatch(actual.to_string()))
    }
}

fn require_matching_container_port(
    labels: &SessionMetadata,
    runtime: RuntimeKind,
    expected: u16,
) -> AttachLabelsResult<()> {
    let actual = require_attach_label(labels, LABEL_CONTAINER_PORT)?
        .parse::<u16>()
        .map_err(|error| AttachLabelError::MalformedContainerPort(error.to_string()))?;

    if actual == expected {
        Ok(())
    } else {
        Err(AttachLabelError::ContainerPortMismatch {
            runtime,
            actual,
            expected,
        })
    }
}
