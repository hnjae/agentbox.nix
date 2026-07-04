// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::lock::{WorkspaceLockFile, WorkspaceLockFileStatus};
use crate::podman::PodmanVolume;
use crate::runtime::RuntimeKind;
use crate::workspace::is_agentbox_workspace_resource_name;

use super::inventory::{CleanInventory, DefaultRuntimeImageCandidate};
use super::resource::{CleanResource, ResourceKind};
use super::scope::CleanScope;
use super::usage::ResourceUsage;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CleanPlan {
    pub(super) candidates: Vec<CleanCandidate>,
    pub(super) skipped: Vec<SkippedResource>,
}

impl CleanPlan {
    pub(super) fn from_inventory(scope: &CleanScope, inventory: &CleanInventory) -> Self {
        let usage = ResourceUsage::from_containers(&inventory.containers);
        let mut plan = CleanPlan::default();

        if scope.includes(ResourceKind::Image) {
            add_default_runtime_image_candidates(
                &inventory.default_runtime_images,
                &usage,
                &mut plan,
            );
        }

        if scope.includes(ResourceKind::Volume) {
            add_cache_volume_candidates(&inventory.volumes, &usage, &mut plan);
        }

        if scope.includes(ResourceKind::LockFile) {
            add_lock_file_candidates(&inventory.lock_files, &mut plan);
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
    WorkspaceLockFile {
        path: PathBuf,
        resource: CleanResource,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SkippedResource {
    pub(super) resource: CleanResource,
    pub(super) reason: String,
}

impl CleanCandidate {
    pub(super) fn default_runtime_image(runtime: RuntimeKind, resource: CleanResource) -> Self {
        Self::DefaultRuntimeImage { runtime, resource }
    }

    pub(super) fn cache_volume(resource: CleanResource) -> Self {
        Self::CacheVolume { resource }
    }

    pub(super) fn workspace_lock_file(path: PathBuf, resource: CleanResource) -> Self {
        Self::WorkspaceLockFile { path, resource }
    }

    pub(super) fn resource(&self) -> &CleanResource {
        match self {
            Self::DefaultRuntimeImage { resource, .. }
            | Self::CacheVolume { resource }
            | Self::WorkspaceLockFile { resource, .. } => resource,
        }
    }

    pub(super) fn kind(&self) -> ResourceKind {
        self.resource().kind()
    }

    pub(super) fn name(&self) -> &str {
        self.resource().name()
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

fn add_lock_file_candidates(candidates: &[WorkspaceLockFileStatus], plan: &mut CleanPlan) {
    for candidate in candidates {
        match candidate {
            WorkspaceLockFileStatus::Available(file) => add_lock_file_candidate(file, plan),
            WorkspaceLockFileStatus::Locked(file) => plan.skipped.push(SkippedResource {
                resource: lock_file_resource(file),
                reason: "locked by another agentbox process".to_string(),
            }),
        }
    }
}

fn add_lock_file_candidate(file: &WorkspaceLockFile, plan: &mut CleanPlan) {
    plan.candidates.push(CleanCandidate::workspace_lock_file(
        file.path().to_path_buf(),
        lock_file_resource(file),
    ));
}

fn lock_file_resource(file: &WorkspaceLockFile) -> CleanResource {
    CleanResource::lock_file(file.path().display().to_string())
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
    use super::*;
    use crate::podman::{PodmanContainerInspect, PodmanContainerMount, PodmanContainerMountKind};

    const USED_VOLUME: &str = "agentbox-used-abcdef123456";
    const UNUSED_VOLUME: &str = "agentbox-unused-abcdef123456";

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
            lock_files: Vec::new(),
        };

        let plan =
            CleanPlan::from_inventory(&CleanScope::from_flags(true, true, false), &inventory);

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

        let plan =
            CleanPlan::from_inventory(&CleanScope::from_flags(true, false, false), &inventory);

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

    fn volume(name: &str) -> PodmanVolume {
        PodmanVolume {
            name: name.to_string(),
            ..PodmanVolume::default()
        }
    }
}
