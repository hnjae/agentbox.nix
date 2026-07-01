// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use crate::podman::PodmanContainerInspect;

use super::resource::CleanResource;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ResourceUsage {
    users: BTreeMap<CleanResource, String>,
}

impl ResourceUsage {
    pub(super) fn from_containers(containers: &[PodmanContainerInspect]) -> Self {
        let mut usage = Self::default();

        for container in containers {
            usage.mark_image_used(&container.image_name, &container.id);

            for mount in &container.mounts {
                if let Some(volume) = mount.named_volume_name() {
                    usage.mark_volume_used(volume, &container.id);
                }
            }
        }

        usage
    }

    fn mark_image_used(&mut self, image: &str, container_id: &str) {
        self.mark_used(CleanResource::image(image), container_id);
    }

    fn mark_volume_used(&mut self, volume: &str, container_id: &str) {
        self.mark_used(CleanResource::volume(volume), container_id);
    }

    fn mark_used(&mut self, resource: CleanResource, container_id: &str) {
        self.users
            .entry(resource)
            .or_insert_with(|| container_id.to_string());
    }

    pub(super) fn user(&self, resource: &CleanResource) -> Option<&str> {
        self.users.get(resource).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use crate::podman::{
        PodmanContainerInspect, PodmanContainerMount, PodmanContainerMountKind,
        PodmanContainerState,
    };

    use super::*;

    const USED_VOLUME: &str = "agentbox-used-abcdef123456";
    const UNUSED_VOLUME: &str = "agentbox-unused-abcdef123456";

    #[test]
    fn resource_usage_indexes_first_container_for_images_and_mount_sources() {
        let containers = vec![
            inspect_container("first", "shared-image", &[USED_VOLUME]),
            inspect_container("second", "shared-image", &[USED_VOLUME, UNUSED_VOLUME]),
        ];

        let usage = ResourceUsage::from_containers(&containers);

        assert_eq!(
            usage.user(&CleanResource::image("shared-image")),
            Some("first")
        );
        assert_eq!(
            usage.user(&CleanResource::volume(USED_VOLUME)),
            Some("first")
        );
        assert_eq!(
            usage.user(&CleanResource::volume(UNUSED_VOLUME)),
            Some("second")
        );
    }

    #[test]
    fn resource_usage_ignores_bind_mount_sources_when_indexing_volumes() {
        let mut container = inspect_container("bind-user", "image", &[]);
        container.mounts.push(PodmanContainerMount {
            kind: PodmanContainerMountKind::Bind,
            name: None,
            source: USED_VOLUME.to_string(),
            destination: "/workspace".to_string(),
            rw: true,
        });

        let usage = ResourceUsage::from_containers(&[container]);

        assert_eq!(usage.user(&CleanResource::volume(USED_VOLUME)), None);
    }

    #[test]
    fn resource_usage_indexes_named_volume_name_before_storage_source() {
        let mut container = inspect_container("named-volume-user", "image", &[]);
        container.mounts.push(PodmanContainerMount {
            kind: PodmanContainerMountKind::Volume,
            name: Some(USED_VOLUME.to_string()),
            source: format!("/storage/volumes/{USED_VOLUME}/_data"),
            destination: "/home/user".to_string(),
            rw: true,
        });

        let usage = ResourceUsage::from_containers(&[container]);

        assert_eq!(
            usage.user(&CleanResource::volume(USED_VOLUME)),
            Some("named-volume-user")
        );
    }

    fn inspect_container(
        id: &str,
        image_name: &str,
        mount_sources: &[&str],
    ) -> PodmanContainerInspect {
        PodmanContainerInspect {
            id: id.to_string(),
            state: PodmanContainerState {
                status: "running".to_string(),
                running: true,
                pid: 1,
                ..PodmanContainerState::default()
            },
            image_name: image_name.to_string(),
            mounts: mount_sources
                .iter()
                .enumerate()
                .map(|(index, source)| PodmanContainerMount {
                    kind: PodmanContainerMountKind::Volume,
                    name: None,
                    source: (*source).to_string(),
                    destination: format!("/mount/{index}"),
                    rw: true,
                })
                .collect(),
            ..PodmanContainerInspect::default()
        }
    }
}
