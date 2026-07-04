// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CleanResource {
    kind: ResourceKind,
    name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ResourceKind {
    Image,
    Volume,
    LockFile,
}

impl ResourceKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Volume => "volume",
            Self::LockFile => "lock file",
        }
    }
}

impl CleanResource {
    pub(super) fn image(name: impl Into<String>) -> Self {
        Self::new(ResourceKind::Image, name)
    }

    pub(super) fn volume(name: impl Into<String>) -> Self {
        Self::new(ResourceKind::Volume, name)
    }

    pub(super) fn lock_file(name: impl Into<String>) -> Self {
        Self::new(ResourceKind::LockFile, name)
    }

    fn new(kind: ResourceKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
        }
    }

    pub(super) fn kind(&self) -> ResourceKind {
        self.kind
    }

    pub(super) fn name(&self) -> &str {
        &self.name
    }
}
