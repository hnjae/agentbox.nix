// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeSet;

use super::resource::ResourceKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CleanScope {
    resources: BTreeSet<ResourceKind>,
}

impl CleanScope {
    pub(super) fn from_flags(images: bool, volumes: bool, locks: bool) -> Self {
        let resources = if !images && !volumes && !locks {
            BTreeSet::from([
                ResourceKind::Image,
                ResourceKind::Volume,
                ResourceKind::LockFile,
            ])
        } else {
            selected_resources(images, volumes, locks)
        };

        Self { resources }
    }

    pub(super) fn includes(&self, kind: ResourceKind) -> bool {
        self.resources.contains(&kind)
    }
}

fn selected_resources(images: bool, volumes: bool, locks: bool) -> BTreeSet<ResourceKind> {
    let mut resources = BTreeSet::new();

    if images {
        resources.insert(ResourceKind::Image);
    }
    if volumes {
        resources.insert(ResourceKind::Volume);
    }
    if locks {
        resources.insert(ResourceKind::LockFile);
    }

    resources
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_scope_defaults_to_all_resources_when_no_flags_are_selected() {
        let scope = CleanScope::from_flags(false, false, false);

        assert!(scope.includes(ResourceKind::Image));
        assert!(scope.includes(ResourceKind::Volume));
        assert!(scope.includes(ResourceKind::LockFile));
    }

    #[test]
    fn clean_scope_can_select_only_images_volumes_or_locks() {
        let images = CleanScope::from_flags(true, false, false);
        let volumes = CleanScope::from_flags(false, true, false);
        let locks = CleanScope::from_flags(false, false, true);

        assert!(images.includes(ResourceKind::Image));
        assert!(!images.includes(ResourceKind::Volume));
        assert!(!images.includes(ResourceKind::LockFile));
        assert!(!volumes.includes(ResourceKind::Image));
        assert!(volumes.includes(ResourceKind::Volume));
        assert!(!volumes.includes(ResourceKind::LockFile));
        assert!(!locks.includes(ResourceKind::Image));
        assert!(!locks.includes(ResourceKind::Volume));
        assert!(locks.includes(ResourceKind::LockFile));
    }

    #[test]
    fn clean_scope_selects_only_requested_resources_when_multiple_flags_are_selected() {
        let scope = CleanScope::from_flags(true, true, false);

        assert!(scope.includes(ResourceKind::Image));
        assert!(scope.includes(ResourceKind::Volume));
        assert!(!scope.includes(ResourceKind::LockFile));
    }
}
