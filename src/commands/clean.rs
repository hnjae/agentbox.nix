// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use crate::cli::CleanArgs;
use crate::diagnostic;
use crate::podman::{Podman, PodmanContainerInspect};
use crate::prompt;
use crate::runtime::RuntimeKind;
use crate::{Error, Result};

use super::runtime::remove_default_runtime_image_state;

const CACHE_VOLUME_PREFIX: &str = "agentbox-";
const CACHE_VOLUME_HASH_LEN: usize = 12;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum CleanCandidate {
    DefaultRuntimeImage { runtime: RuntimeKind },
    CacheVolume { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkippedResource {
    kind: ResourceKind,
    name: String,
    reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl CleanCandidate {
    fn kind(&self) -> ResourceKind {
        match self {
            Self::DefaultRuntimeImage { .. } => ResourceKind::Image,
            Self::CacheVolume { .. } => ResourceKind::Volume,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::DefaultRuntimeImage { runtime } => runtime.default_image(),
            Self::CacheVolume { name } => name,
        }
    }
}

fn build_clean_plan(podman: &Podman, scope: CleanScope) -> Result<CleanPlan> {
    let containers = inspect_all_containers(podman)?;
    let usage = ResourceUsage::from_containers(&containers);
    let mut plan = CleanPlan::default();

    if scope.images {
        add_default_runtime_image_candidates(podman, &usage, &mut plan)?;
    }

    if scope.volumes {
        add_cache_volume_candidates(podman, &usage, &mut plan)?;
    }

    Ok(plan)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ResourceUsage {
    image_users: BTreeMap<String, String>,
    volume_users: BTreeMap<String, String>,
}

impl ResourceUsage {
    fn from_containers(containers: &[PodmanContainerInspect]) -> Self {
        let mut usage = Self::default();

        for container in containers {
            usage
                .image_users
                .entry(container.image_name.clone())
                .or_insert_with(|| container.id.clone());

            for mount in &container.mounts {
                usage
                    .volume_users
                    .entry(mount.source.clone())
                    .or_insert_with(|| container.id.clone());
            }
        }

        usage
    }

    fn image_user(&self, image: &str) -> Option<&str> {
        self.image_users.get(image).map(String::as_str)
    }

    fn volume_user(&self, volume: &str) -> Option<&str> {
        self.volume_users.get(volume).map(String::as_str)
    }
}

fn add_default_runtime_image_candidates(
    podman: &Podman,
    usage: &ResourceUsage,
    plan: &mut CleanPlan,
) -> Result<()> {
    for runtime in RuntimeKind::variants().iter().copied() {
        if podman.image_exists(runtime.default_image())? {
            add_default_runtime_image_candidate(runtime, usage, plan);
        }
    }

    Ok(())
}

fn add_default_runtime_image_candidate(
    runtime: RuntimeKind,
    usage: &ResourceUsage,
    plan: &mut CleanPlan,
) {
    let image = runtime.default_image();

    if let Some(container_id) = usage.image_user(image) {
        plan.skipped.push(SkippedResource {
            kind: ResourceKind::Image,
            name: image.to_string(),
            reason: format!("used by container `{container_id}`"),
        });
    } else {
        plan.candidates
            .push(CleanCandidate::DefaultRuntimeImage { runtime });
    }
}

fn add_cache_volume_candidates(
    podman: &Podman,
    usage: &ResourceUsage,
    plan: &mut CleanPlan,
) -> Result<()> {
    for volume in podman.volumes()? {
        if is_agentbox_cache_volume_name(&volume.name) {
            add_cache_volume_candidate(volume.name, usage, plan);
        }
    }

    Ok(())
}

fn add_cache_volume_candidate(name: String, usage: &ResourceUsage, plan: &mut CleanPlan) {
    if let Some(container_id) = usage.volume_user(&name) {
        plan.skipped.push(SkippedResource {
            kind: ResourceKind::Volume,
            name,
            reason: format!("mounted by container `{container_id}`"),
        });
    } else {
        plan.candidates.push(CleanCandidate::CacheVolume { name });
    }
}

fn inspect_all_containers(podman: &Podman) -> Result<Vec<PodmanContainerInspect>> {
    podman
        .ps_all()?
        .into_iter()
        .map(|container| podman.inspect_one(&container.id))
        .collect()
}

fn is_agentbox_cache_volume_name(name: &str) -> bool {
    let Some((prefix, suffix)) = name.rsplit_once('-') else {
        return false;
    };

    prefix.starts_with(CACHE_VOLUME_PREFIX)
        && prefix.len() > CACHE_VOLUME_PREFIX.len()
        && suffix.len() == CACHE_VOLUME_HASH_LEN
        && suffix.chars().all(|ch| ch.is_ascii_hexdigit())
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
            resource.kind.as_str(),
            resource.name.as_str(),
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
                kind: candidate.kind(),
                name: candidate.name().to_string(),
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
        CleanCandidate::DefaultRuntimeImage { runtime } => {
            podman.remove_image(runtime.default_image())?;
            remove_default_runtime_image_state(*runtime)?;
            Ok(())
        }
        CleanCandidate::CacheVolume { name } => podman.remove_volume(name),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeleteFailure {
    kind: ResourceKind,
    name: String,
    error: String,
}

fn render_delete_failures(failures: &[DeleteFailure]) -> String {
    let details = failures
        .iter()
        .map(|failure| {
            format!(
                "{} `{}` ({})",
                failure.kind.as_str(),
                failure.name,
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

    use crate::podman::{
        PodmanContainerConfig, PodmanContainerInspect, PodmanContainerMount, PodmanContainerState,
        PodmanHostConfig, PodmanNetworkSettings,
    };
    use crate::runtime::default_image::OPENCODE_DEFAULT_IMAGE;

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

        assert_eq!(usage.image_user("shared-image"), Some("first"));
        assert_eq!(usage.volume_user(USED_VOLUME), Some("first"));
        assert_eq!(usage.volume_user(UNUSED_VOLUME), Some("second"));
    }

    #[test]
    fn clean_plan_helpers_skip_used_resources_and_keep_unused_candidates() {
        let usage = ResourceUsage {
            image_users: BTreeMap::from([(
                OPENCODE_DEFAULT_IMAGE.to_string(),
                "running-opencode".to_string(),
            )]),
            volume_users: BTreeMap::from([(
                USED_VOLUME.to_string(),
                "running-opencode".to_string(),
            )]),
        };
        let mut plan = CleanPlan::default();

        add_default_runtime_image_candidate(RuntimeKind::Opencode, &usage, &mut plan);
        add_default_runtime_image_candidate(RuntimeKind::Codex, &usage, &mut plan);
        add_cache_volume_candidate(USED_VOLUME.to_string(), &usage, &mut plan);
        add_cache_volume_candidate(UNUSED_VOLUME.to_string(), &usage, &mut plan);

        assert_eq!(
            plan.candidates,
            vec![
                CleanCandidate::DefaultRuntimeImage {
                    runtime: RuntimeKind::Codex,
                },
                CleanCandidate::CacheVolume {
                    name: UNUSED_VOLUME.to_string(),
                },
            ]
        );
        assert_eq!(
            plan.skipped,
            vec![
                SkippedResource {
                    kind: ResourceKind::Image,
                    name: OPENCODE_DEFAULT_IMAGE.to_string(),
                    reason: "used by container `running-opencode`".to_string(),
                },
                SkippedResource {
                    kind: ResourceKind::Volume,
                    name: USED_VOLUME.to_string(),
                    reason: "mounted by container `running-opencode`".to_string(),
                },
            ]
        );
    }

    #[test]
    fn cache_volume_name_filter_requires_agentbox_prefix_and_hash_suffix() {
        assert!(is_agentbox_cache_volume_name(UNUSED_VOLUME));

        for name in [
            "agentbox-data",
            "agentbox-short-abc123",
            "other-agentbox-abcdef123456",
            "agentbox-used-xyzxyzxyzxyz",
            "agentbox--abcdef123456",
        ] {
            assert!(
                !is_agentbox_cache_volume_name(name),
                "`{name}` should not be treated as an agentbox cache volume",
            );
        }
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
}
