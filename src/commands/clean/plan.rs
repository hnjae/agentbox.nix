// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};

use crate::podman::{PodmanContainerInspect, PodmanVolume};
use crate::runtime::RuntimeKind;
use crate::workspace::is_agentbox_workspace_resource_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CleanScope {
    pub(super) images: bool,
    pub(super) volumes: bool,
}

impl CleanScope {
    pub(super) fn from_flags(images: bool, volumes: bool) -> Self {
        Self {
            images: images || !volumes,
            volumes: volumes || !images,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CleanPlan {
    pub(super) candidates: Vec<CleanCandidate>,
    pub(super) skipped: Vec<SkippedResource>,
}

impl CleanPlan {
    pub(super) fn from_inventory(scope: CleanScope, inventory: &CleanInventory) -> Self {
        let usage = ResourceUsage::from_containers(&inventory.containers);
        let mut plan = CleanPlan::default();

        if scope.images {
            add_default_runtime_image_candidates(
                &inventory.default_runtime_images,
                &usage,
                &mut plan,
            );
        }

        if scope.volumes {
            add_cache_volume_candidates(&inventory.volumes, &usage, &mut plan);
        }

        plan
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CleanCandidate {
    DefaultRuntimeImage {
        runtime: RuntimeKind,
        resource: CleanResource,
    },
    CacheVolume {
        resource: CleanResource,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SkippedResource {
    pub(super) resource: CleanResource,
    pub(super) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CleanResource {
    kind: ResourceKind,
    name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ResourceKind {
    Image,
    Volume,
}

impl ResourceKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Volume => "volume",
        }
    }
}

impl CleanResource {
    fn image(name: impl Into<String>) -> Self {
        Self::new(ResourceKind::Image, name)
    }

    fn volume(name: impl Into<String>) -> Self {
        Self::new(ResourceKind::Volume, name)
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

impl CleanCandidate {
    fn default_runtime_image(runtime: RuntimeKind, resource: CleanResource) -> Self {
        Self::DefaultRuntimeImage { runtime, resource }
    }

    fn cache_volume(resource: CleanResource) -> Self {
        Self::CacheVolume { resource }
    }

    pub(super) fn resource(&self) -> &CleanResource {
        match self {
            Self::DefaultRuntimeImage { resource, .. } | Self::CacheVolume { resource } => resource,
        }
    }

    pub(super) fn kind(&self) -> ResourceKind {
        self.resource().kind()
    }

    pub(super) fn name(&self) -> &str {
        self.resource().name()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CleanInventory {
    pub(super) containers: Vec<PodmanContainerInspect>,
    pub(super) default_runtime_images: Vec<DefaultRuntimeImageCandidate>,
    pub(super) volumes: Vec<PodmanVolume>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DefaultRuntimeImageCandidate {
    pub(super) runtime: RuntimeKind,
    pub(super) image: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ResourceUsage {
    users: BTreeMap<CleanResource, String>,
}

impl ResourceUsage {
    fn from_containers(containers: &[PodmanContainerInspect]) -> Self {
        let mut usage = Self::default();

        for container in containers {
            usage.mark_image_used(&container.image_name, &container.id);

            for mount in &container.mounts {
                if mount.kind.is_volume() {
                    usage.mark_volume_used(&mount.source, &container.id);
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

    fn user(&self, resource: &CleanResource) -> Option<&str> {
        self.users.get(resource).map(String::as_str)
    }
}

fn add_default_runtime_image_candidates(
    candidates: &[DefaultRuntimeImageCandidate],
    usage: &ResourceUsage,
    plan: &mut CleanPlan,
) {
    let mut seen = BTreeSet::new();

    for candidate in candidates {
        if seen.insert(candidate.image.clone()) {
            add_default_runtime_image_candidate(
                candidate.runtime,
                candidate.image.clone(),
                usage,
                plan,
            );
        }
    }
}

fn add_default_runtime_image_candidate(
    runtime: RuntimeKind,
    image: String,
    usage: &ResourceUsage,
    plan: &mut CleanPlan,
) {
    add_candidate_or_skip(
        CleanResource::image(image),
        usage,
        "used",
        |resource| CleanCandidate::default_runtime_image(runtime, resource),
        plan,
    );
}

fn add_cache_volume_candidates(
    volumes: &[PodmanVolume],
    usage: &ResourceUsage,
    plan: &mut CleanPlan,
) {
    for volume in volumes {
        if is_agentbox_workspace_resource_name(&volume.name) {
            add_cache_volume_candidate(volume.name.clone(), usage, plan);
        }
    }
}

fn add_cache_volume_candidate(name: String, usage: &ResourceUsage, plan: &mut CleanPlan) {
    add_candidate_or_skip(
        CleanResource::volume(name),
        usage,
        "mounted",
        CleanCandidate::cache_volume,
        plan,
    );
}

fn add_candidate_or_skip(
    resource: CleanResource,
    usage: &ResourceUsage,
    used_action: &str,
    candidate: impl FnOnce(CleanResource) -> CleanCandidate,
    plan: &mut CleanPlan,
) {
    if let Some(container_id) = usage.user(&resource) {
        plan.skipped.push(SkippedResource {
            resource,
            reason: format!("{used_action} by container `{container_id}`"),
        });
    } else {
        plan.candidates.push(candidate(resource));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::podman::{
        PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount,
        PodmanContainerMountKind, PodmanContainerState, PodmanHostConfig, PodmanNetworkSettings,
    };

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
            source: USED_VOLUME.to_string(),
            destination: "/workspace".to_string(),
            rw: true,
        });

        let usage = ResourceUsage::from_containers(&[container]);

        assert_eq!(usage.user(&CleanResource::volume(USED_VOLUME)), None);
    }

    #[test]
    fn clean_plan_from_inventory_skips_used_resources_and_keeps_unused_candidates() {
        let opencode_image = RuntimeKind::Opencode.default_image();
        let codex_image = RuntimeKind::Codex.default_image();
        let inventory = CleanInventory {
            containers: vec![inspect_container(
                "running-opencode",
                &opencode_image,
                &[USED_VOLUME],
            )],
            default_runtime_images: vec![
                DefaultRuntimeImageCandidate {
                    runtime: RuntimeKind::Opencode,
                    image: opencode_image.clone(),
                },
                DefaultRuntimeImageCandidate {
                    runtime: RuntimeKind::Codex,
                    image: codex_image.clone(),
                },
            ],
            volumes: vec![volume(USED_VOLUME), volume(UNUSED_VOLUME)],
        };

        let plan = CleanPlan::from_inventory(
            CleanScope {
                images: true,
                volumes: true,
            },
            &inventory,
        );

        assert_eq!(
            plan.candidates,
            vec![
                CleanCandidate::default_runtime_image(
                    RuntimeKind::Codex,
                    CleanResource::image(codex_image),
                ),
                CleanCandidate::cache_volume(CleanResource::volume(UNUSED_VOLUME)),
            ]
        );
        assert_eq!(
            plan.skipped,
            vec![
                SkippedResource {
                    resource: CleanResource::image(opencode_image),
                    reason: "used by container `running-opencode`".to_string(),
                },
                SkippedResource {
                    resource: CleanResource::volume(USED_VOLUME),
                    reason: "mounted by container `running-opencode`".to_string(),
                },
            ]
        );
    }

    #[test]
    fn clean_plan_deduplicates_duplicate_image_references() {
        let image = RuntimeKind::Opencode.default_image();
        let inventory = CleanInventory {
            default_runtime_images: vec![
                DefaultRuntimeImageCandidate {
                    runtime: RuntimeKind::Opencode,
                    image: image.clone(),
                },
                DefaultRuntimeImageCandidate {
                    runtime: RuntimeKind::Opencode,
                    image: image.clone(),
                },
            ],
            ..CleanInventory::default()
        };

        let plan = CleanPlan::from_inventory(
            CleanScope {
                images: true,
                volumes: false,
            },
            &inventory,
        );

        assert_eq!(
            plan.candidates,
            vec![CleanCandidate::default_runtime_image(
                RuntimeKind::Opencode,
                CleanResource::image(image)
            )]
        );
    }

    fn inspect_container(
        id: &str,
        image_name: &str,
        mount_sources: &[&str],
    ) -> PodmanContainerInspect {
        PodmanContainerInspect {
            id: id.to_string(),
            created: String::new(),
            path: String::new(),
            args: Vec::new(),
            state: PodmanContainerState {
                status: "running".to_string(),
                running: true,
                exit_code: 0,
                pid: 1,
                started_at: None,
                finished_at: None,
                health: None,
            },
            image_name: image_name.to_string(),
            config: PodmanContainerConfig {
                user: None,
                env: Vec::new(),
                cmd: Vec::new(),
                working_dir: None,
                labels: BTreeMap::new(),
                entrypoint: None,
                stop_signal: None,
            },
            host_config: PodmanHostConfig {
                auto_remove: false,
                network_mode: None,
                privileged: false,
            },
            mounts: mount_sources
                .iter()
                .enumerate()
                .map(|(index, source)| PodmanContainerMount {
                    kind: PodmanContainerMountKind::Volume,
                    source: (*source).to_string(),
                    destination: format!("/mount/{index}"),
                    rw: true,
                })
                .collect(),
            network_settings: PodmanNetworkSettings {
                networks: BTreeMap::new(),
                ports: BTreeMap::new(),
            },
        }
    }

    fn volume(name: &str) -> PodmanVolume {
        PodmanVolume {
            name: name.to_string(),
            driver: None,
            mountpoint: None,
            created_at: None,
            labels: BTreeMap::new(),
            scope: None,
            options: BTreeMap::new(),
        }
    }
}
