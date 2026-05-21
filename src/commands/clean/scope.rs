// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeSet;

use super::resource::ResourceKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CleanScope {
    resources: BTreeSet<ResourceKind>,
}

impl CleanScope {
    pub(super) fn from_flags(images: bool, volumes: bool) -> Self {
        let resources = match (images, volumes) {
            (false, false) | (true, true) => {
                BTreeSet::from([ResourceKind::Image, ResourceKind::Volume])
            }
            (true, false) => BTreeSet::from([ResourceKind::Image]),
            (false, true) => BTreeSet::from([ResourceKind::Volume]),
        };

        Self { resources }
    }

    pub(super) fn includes(&self, kind: ResourceKind) -> bool {
        self.resources.contains(&kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_scope_defaults_to_all_resources_when_no_flags_are_selected() {
        let scope = CleanScope::from_flags(false, false);

        assert!(scope.includes(ResourceKind::Image));
        assert!(scope.includes(ResourceKind::Volume));
    }

    #[test]
    fn clean_scope_can_select_only_images_or_only_volumes() {
        let images = CleanScope::from_flags(true, false);
        let volumes = CleanScope::from_flags(false, true);

        assert!(images.includes(ResourceKind::Image));
        assert!(!images.includes(ResourceKind::Volume));
        assert!(!volumes.includes(ResourceKind::Image));
        assert!(volumes.includes(ResourceKind::Volume));
    }

    #[test]
    fn clean_scope_selects_all_resources_when_both_flags_are_selected() {
        let scope = CleanScope::from_flags(true, true);

        assert!(scope.includes(ResourceKind::Image));
        assert!(scope.includes(ResourceKind::Volume));
    }
}
