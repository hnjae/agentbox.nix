// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::Result;
use crate::metadata::{DefaultRuntimeImageMetadata, default_runtime_image_label_filter};
use crate::podman::{Podman, PodmanContainerInspect, PodmanImage};
use crate::runtime::default_image;

use super::plan::{CleanInventory, CleanScope, DefaultRuntimeImageCandidate};

impl CleanInventory {
    pub(super) fn from_podman(podman: &Podman, scope: CleanScope) -> Result<Self> {
        let containers = inspect_all_containers(podman)?;
        let default_runtime_images = if scope.images {
            default_runtime_image_candidates(podman)?
        } else {
            Vec::new()
        };
        let volumes = if scope.volumes {
            podman.volumes()?
        } else {
            Vec::new()
        };

        Ok(Self {
            containers,
            default_runtime_images,
            volumes,
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
