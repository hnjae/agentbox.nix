// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::metadata::{
    LABEL_ATTACH_SCHEME, LABEL_CONTAINER_LISTEN_IP, LABEL_CONTAINER_PORT, LABEL_RUNTIME,
};
use crate::runtime::{RuntimeAttachSpec, RuntimeKind};
use crate::{Error, session::record::SessionMetadata, session::status::SessionFailure};

use super::AttachLabelsResult;

#[derive(Debug)]
pub(in crate::session) enum AttachLabelError {
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

    pub(in crate::session) fn into_error(self) -> Error {
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
pub(in crate::session) struct AttachLabels {
    runtime: RuntimeKind,
    attach: RuntimeAttachSpec,
}

impl AttachLabels {
    pub(super) fn from_session_labels(labels: &SessionMetadata) -> AttachLabelsResult<Self> {
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

    pub(in crate::session) fn attach_spec(self) -> RuntimeAttachSpec {
        self.attach
    }
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
