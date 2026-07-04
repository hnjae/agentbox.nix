// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::lock::{WorkspaceLockFileStatus, cleanup_lock_files};
use crate::metadata::{DefaultRuntimeImageMetadata, default_runtime_image_label_filter};
use crate::podman::{Podman, PodmanContainerInspect, PodmanImage, PodmanVolume};
use crate::runtime::RuntimeKind;
use crate::runtime::default_image;

use super::resource::ResourceKind;
use super::scope::CleanScope;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CleanInventory {
    pub(super) containers: Vec<PodmanContainerInspect>,
    pub(super) default_runtime_images: Vec<DefaultRuntimeImageCandidate>,
    pub(super) volumes: Vec<PodmanVolume>,
    pub(super) lock_files: Vec<WorkspaceLockFileStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DefaultRuntimeImageCandidate {
    pub(super) runtime: RuntimeKind,
    pub(super) image: String,
}

impl CleanInventory {
    pub(super) fn from_podman(podman: &Podman, scope: &CleanScope) -> Result<Self> {
        let containers =
            if scope.includes(ResourceKind::Image) || scope.includes(ResourceKind::Volume) {
                inspect_all_containers(podman)?
            } else {
                Vec::new()
            };
        let default_runtime_images = if scope.includes(ResourceKind::Image) {
            default_runtime_image_candidates(podman)?
        } else {
            Vec::new()
        };
        let volumes = if scope.includes(ResourceKind::Volume) {
            podman.volumes()?
        } else {
            Vec::new()
        };
        let lock_files = if scope.includes(ResourceKind::LockFile) {
            cleanup_lock_files()?
        } else {
            Vec::new()
        };

        Ok(Self {
            containers,
            default_runtime_images,
            volumes,
            lock_files,
        })
    }
}

fn inspect_all_containers(podman: &Podman) -> Result<Vec<PodmanContainerInspect>> {
    podman
        .ps_all()?
        .into_iter()
        .map(|container| podman.inspect_one(&container.id))
        .collect()
}

fn default_runtime_image_candidates(podman: &Podman) -> Result<Vec<DefaultRuntimeImageCandidate>> {
    labeled_default_runtime_images(podman)
}

fn labeled_default_runtime_images(podman: &Podman) -> Result<Vec<DefaultRuntimeImageCandidate>> {
    let images = podman.images_with_label(&default_runtime_image_label_filter())?;
    Ok(images
        .iter()
        .flat_map(labeled_default_runtime_image_candidates)
        .collect())
}

fn labeled_default_runtime_image_candidates(
    image: &PodmanImage,
) -> Vec<DefaultRuntimeImageCandidate> {
    let Some(metadata) = DefaultRuntimeImageMetadata::from_labels(&image.labels) else {
        return Vec::new();
    };
    let runtime = metadata.runtime();
    let context_hash = metadata.image_context_hash();

    image
        .references()
        .into_iter()
        .filter(|reference| {
            default_image::is_content_hash_default_image_ref(runtime, reference)
                && reference.ends_with(context_hash)
        })
        .map(|image| DefaultRuntimeImageCandidate { runtime, image })
        .collect()
}
