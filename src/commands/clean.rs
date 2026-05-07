// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::{BTreeMap, BTreeSet};

use crate::cli::CleanArgs;
use crate::diagnostic;
use crate::metadata::{DefaultRuntimeImageMetadata, default_runtime_image_label_filter};
use crate::podman::{Podman, PodmanContainerInspect, PodmanImage, PodmanVolume};
use crate::prompt;
use crate::runtime::{RuntimeKind, default_image};
use crate::workspace::is_agentbox_workspace_resource_name;
use crate::{Error, Result};

use super::runtime::remove_default_runtime_image_state_if_image;

pub fn run(args: CleanArgs) -> Result<()> {
    let podman = Podman::new();
    let scope = CleanScope::from_args(&args);
    let plan = build_clean_plan(&podman, scope)?;

    if args.dry_run {
        diagnostic::info(render_plan(&plan));
        return Ok(());
    }

    if plan.candidates.is_empty() {
        diagnostic::info("nothing to clean");
        return Ok(());
    }

    diagnostic::info(render_plan(&plan));
    if !args.yes && !confirm_interactive()? {
        diagnostic::warning("aborted");
        return Ok(());
    }

    apply_clean_plan(&podman, &plan)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CleanScope {
    images: bool,
    volumes: bool,
}

impl CleanScope {
    fn from_args(args: &CleanArgs) -> Self {
        Self {
            images: args.images || !args.volumes,
            volumes: args.volumes || !args.images,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CleanPlan {
    candidates: Vec<CleanCandidate>,
    skipped: Vec<SkippedResource>,
}

impl CleanPlan {
    fn from_inventory(scope: CleanScope, inventory: &CleanInventory) -> Self {
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
enum CleanCandidate {
    DefaultRuntimeImage {
        runtime: RuntimeKind,
        resource: CleanResource,
    },
    CacheVolume {
        resource: CleanResource,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkippedResource {
    resource: CleanResource,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CleanResource {
    kind: ResourceKind,
    name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ResourceKind {
    Image,
    Volume,
}

impl ResourceKind {
    fn as_str(self) -> &'static str {
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

    fn kind(&self) -> ResourceKind {
        self.kind
    }

    fn name(&self) -> &str {
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

    fn resource(&self) -> &CleanResource {
        match self {
            Self::DefaultRuntimeImage { resource, .. } | Self::CacheVolume { resource } => resource,
        }
    }

    fn kind(&self) -> ResourceKind {
        self.resource().kind()
    }

    fn name(&self) -> &str {
        self.resource().name()
    }
}

fn build_clean_plan(podman: &Podman, scope: CleanScope) -> Result<CleanPlan> {
    let inventory = CleanInventory::from_podman(podman, scope)?;

    Ok(CleanPlan::from_inventory(scope, &inventory))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CleanInventory {
    containers: Vec<PodmanContainerInspect>,
    default_runtime_images: Vec<DefaultRuntimeImageCandidate>,
    volumes: Vec<PodmanVolume>,
}

impl CleanInventory {
    fn from_podman(podman: &Podman, scope: CleanScope) -> Result<Self> {
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ResourceUsage {
    users: BTreeMap<CleanResource, String>,
}

impl ResourceUsage {
    fn from_containers(containers: &[PodmanContainerInspect]) -> Self {
        let mut usage = Self::default();

        for container in containers {
            usage.mark_used(CleanResource::image(&container.image_name), &container.id);

            for mount in &container.mounts {
                usage.mark_used(CleanResource::volume(&mount.source), &container.id);
            }
        }

        usage
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

fn default_runtime_image_candidates(podman: &Podman) -> Result<Vec<DefaultRuntimeImageCandidate>> {
    let mut candidates = labeled_default_runtime_images(podman)?;
    candidates.extend(legacy_default_runtime_image_candidates(podman)?);

    Ok(candidates)
}

fn legacy_default_runtime_image_candidates(
    podman: &Podman,
) -> Result<Vec<DefaultRuntimeImageCandidate>> {
    let mut candidates = Vec::new();
    for runtime in RuntimeKind::variants().iter().copied() {
        let image = default_image::legacy_default_image(runtime);
        if podman.image_exists(image)? {
            candidates.push(DefaultRuntimeImageCandidate {
                runtime,
                image: image.to_string(),
            });
        }
    }

    Ok(candidates)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DefaultRuntimeImageCandidate {
    runtime: RuntimeKind,
    image: String,
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

fn inspect_all_containers(podman: &Podman) -> Result<Vec<PodmanContainerInspect>> {
    podman
        .ps_all()?
        .into_iter()
        .map(|container| podman.inspect_one(&container.id))
        .collect()
}

fn render_plan(plan: &CleanPlan) -> String {
    let mut lines = Vec::new();

    if !plan.candidates.is_empty() {
        lines.push("cleanup candidates:".to_string());
        lines.extend(
            plan.candidates
                .iter()
                .map(|candidate| format!("- {} `{}`", candidate.kind().as_str(), candidate.name())),
        );
    }

    if !plan.skipped.is_empty() {
        lines.push("skipped:".to_string());
        lines.extend(skipped_lines(&plan.skipped));
    }

    if lines.is_empty() {
        "nothing to clean\n".to_string()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn skipped_lines(skipped: &[SkippedResource]) -> impl Iterator<Item = String> + '_ {
    skipped.iter().map(|resource| {
        format!(
            "- {} `{}`: {}",
            resource.resource.kind().as_str(),
            resource.resource.name(),
            resource.reason
        )
    })
}

fn confirm_interactive() -> Result<bool> {
    prompt::confirm(
        "Proceed?",
        false,
        "agentbox clean requires --yes or --dry-run when stdin or stderr is not a TTY",
    )
}

fn apply_clean_plan(podman: &Podman, plan: &CleanPlan) -> Result<()> {
    let mut failures = Vec::new();

    for candidate in &plan.candidates {
        match remove_candidate(podman, candidate) {
            Ok(()) => diagnostic::info(format!(
                "removed {} `{}`",
                candidate.kind().as_str(),
                candidate.name()
            )),
            Err(error) => failures.push(DeleteFailure {
                resource: candidate.resource().clone(),
                error: error.to_string(),
            }),
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(render_delete_failures(&failures)))
    }
}

fn remove_candidate(podman: &Podman, candidate: &CleanCandidate) -> Result<()> {
    match candidate {
        CleanCandidate::DefaultRuntimeImage { runtime, resource } => {
            podman.remove_image(resource.name())?;
            remove_default_runtime_image_state_if_image(*runtime, resource.name())?;
            Ok(())
        }
        CleanCandidate::CacheVolume { resource } => podman.remove_volume(resource.name()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeleteFailure {
    resource: CleanResource,
    error: String,
}

fn render_delete_failures(failures: &[DeleteFailure]) -> String {
    let details = failures
        .iter()
        .map(|failure| {
            format!(
                "{} `{}` ({})",
                failure.resource.kind().as_str(),
                failure.resource.name(),
                failure.error
            )
        })
        .collect::<Vec<_>>()
        .join("; ");

    format!("partial clean failed; failed resources: {details}")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::podman::{
        PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount, PodmanContainerState,
        PodmanHostConfig, PodmanNetworkSettings,
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
    fn clean_plan_deduplicates_labeled_and_legacy_image_references() {
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
                    kind: "volume".to_string(),
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
