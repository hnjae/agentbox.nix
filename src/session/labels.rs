// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod attach;
mod identity;

pub(super) use attach::{AttachLabelError, AttachLabels};
pub(super) use identity::{RequiredSessionLabels, SessionIdentityLabels};

use super::record::SessionMetadata;
use super::status::SessionFailure;

type SessionLabelResult<T> = std::result::Result<T, SessionFailure>;
type AttachLabelsResult<T> = std::result::Result<T, AttachLabelError>;

#[derive(Debug)]
pub(super) struct SessionLabelReport {
    identity: SessionLabelResult<SessionIdentityLabels>,
    required: SessionLabelResult<RequiredSessionLabels>,
    attach: AttachLabelsResult<AttachLabels>,
}

impl SessionLabelReport {
    pub(super) fn from_metadata(metadata: &SessionMetadata) -> Self {
        let identity = SessionIdentityLabels::validated(metadata);
        let required = RequiredSessionLabels::from_validated_identity(metadata, identity.clone());

        Self {
            identity,
            required,
            attach: AttachLabels::from_session_labels(metadata),
        }
    }

    pub(super) fn identity_labels(&self) -> SessionLabelResult<&SessionIdentityLabels> {
        self.identity.as_ref().map_err(|failure| *failure)
    }

    pub(super) fn required_labels(&self) -> SessionLabelResult<&RequiredSessionLabels> {
        self.required.as_ref().map_err(|failure| *failure)
    }

    pub(super) fn required_failure(&self) -> Option<SessionFailure> {
        self.required.as_ref().err().copied()
    }

    pub(super) fn attach_failure(&self) -> Option<SessionFailure> {
        self.attach_labels().err()
    }

    pub(super) fn attach_labels(&self) -> SessionLabelResult<AttachLabels> {
        self.attach
            .as_ref()
            .copied()
            .map_err(|error| error.session_failure())
    }
}

impl SessionMetadata {
    pub(super) fn attach_labels(&self) -> AttachLabelsResult<AttachLabels> {
        AttachLabels::from_session_labels(self)
    }
}

fn require_session_label<'a>(
    labels: &'a SessionMetadata,
    name: &'static str,
) -> SessionLabelResult<&'a str> {
    labels
        .label(name)
        .ok_or(SessionFailure::MissingRequiredLabels)
}

fn require_session_label_value(
    labels: &SessionMetadata,
    name: &'static str,
    expected: &'static str,
) -> SessionLabelResult<()> {
    match labels.label(name) {
        Some(actual) if actual == expected => Ok(()),
        _ => Err(SessionFailure::MissingRequiredLabels),
    }
}

fn require_session_labels(
    labels: &SessionMetadata,
    names: &[&'static str],
) -> SessionLabelResult<()> {
    for name in names {
        require_session_label(labels, name)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use camino::Utf8Path;

    use super::*;
    use crate::metadata::{
        LABEL_GIT_ROOT_HASH, LABEL_LAUNCH_DIRECTORY, ManagedSessionLabelInput,
        managed_session_labels,
    };
    use crate::runtime::RuntimeKind;
    use crate::workspace::git_root_hash12;

    const GIT_ROOT: &str = "/workspace/project";
    const CONTAINER_NAME: &str = "agentbox-project";

    #[test]
    fn report_propagates_identity_failure_to_required_labels() {
        let mut labels = valid_labels();
        labels.insert(
            LABEL_GIT_ROOT_HASH.to_string(),
            "not-the-current-hash".to_string(),
        );
        let metadata = SessionMetadata::from_labels(&labels);
        let report = SessionLabelReport::from_metadata(&metadata);

        assert_eq!(
            report.identity_labels().unwrap_err(),
            SessionFailure::DriftedGitRootHash
        );
        assert_eq!(
            report.required_failure(),
            Some(SessionFailure::DriftedGitRootHash)
        );
    }

    #[test]
    fn report_keeps_required_metadata_failures_separate_from_identity() {
        let mut labels = valid_labels();
        labels.insert(LABEL_LAUNCH_DIRECTORY.to_string(), "/outside".to_string());
        let metadata = SessionMetadata::from_labels(&labels);
        let report = SessionLabelReport::from_metadata(&metadata);

        assert!(report.identity_labels().is_ok());
        assert_eq!(
            report.required_failure(),
            Some(SessionFailure::MalformedLaunchDirectory)
        );
    }

    fn valid_labels() -> BTreeMap<String, String> {
        let git_root_hash = git_root_hash12(Utf8Path::new(GIT_ROOT));
        managed_session_labels(ManagedSessionLabelInput {
            canonical_git_root: GIT_ROOT,
            git_root_hash: git_root_hash.as_str(),
            runtime: RuntimeKind::Opencode,
            image: "localhost/agentbox-opencode:ctx-0123456789abcdef",
            launch_directory: GIT_ROOT,
            logical_name: CONTAINER_NAME,
        })
    }
}
