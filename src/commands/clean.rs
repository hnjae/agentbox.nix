// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::cli::CleanArgs;
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
        print!("{}", render_plan(&plan));
        return Ok(());
    }

    if plan.candidates.is_empty() {
        println!("nothing to clean");
        return Ok(());
    }

    print!("{}", render_plan(&plan));
    if !args.yes && !confirm_interactive()? {
        eprintln!("aborted");
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct CleanPlan {
    candidates: Vec<CleanCandidate>,
    skipped: Vec<SkippedResource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CleanCandidate {
    kind: ResourceKind,
    name: String,
    runtime: Option<RuntimeKind>,
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

fn build_clean_plan(podman: &Podman, scope: CleanScope) -> Result<CleanPlan> {
    let containers = inspect_all_containers(podman)?;
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();

    if scope.images {
        for runtime in [RuntimeKind::Opencode, RuntimeKind::Codex] {
            let image = runtime.default_image();
            if !podman.image_exists(image)? {
                continue;
            }

            if let Some(container) = containers
                .iter()
                .find(|container| container.image_name == image)
            {
                skipped.push(SkippedResource {
                    kind: ResourceKind::Image,
                    name: image.to_string(),
                    reason: format!("used by container `{}`", container.id),
                });
            } else {
                candidates.push(CleanCandidate {
                    kind: ResourceKind::Image,
                    name: image.to_string(),
                    runtime: Some(runtime),
                });
            }
        }
    }

    if scope.volumes {
        for volume in podman.volumes()? {
            if !is_agentbox_cache_volume_name(&volume.name) {
                continue;
            }

            if let Some(container) = containers.iter().find(|container| {
                container
                    .mounts
                    .iter()
                    .any(|mount| mount.source == volume.name)
            }) {
                skipped.push(SkippedResource {
                    kind: ResourceKind::Volume,
                    name: volume.name,
                    reason: format!("mounted by container `{}`", container.id),
                });
            } else {
                candidates.push(CleanCandidate {
                    kind: ResourceKind::Volume,
                    name: volume.name,
                    runtime: None,
                });
            }
        }
    }

    Ok(CleanPlan {
        candidates,
        skipped,
    })
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
        lines.extend(plan.candidates.iter().map(|candidate| {
            format!(
                "- {} `{}`",
                candidate.kind.as_str(),
                candidate.name.as_str()
            )
        }));
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
            Ok(()) => println!("removed {} `{}`", candidate.kind.as_str(), candidate.name),
            Err(error) => failures.push(DeleteFailure {
                kind: candidate.kind,
                name: candidate.name.clone(),
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
    match candidate.kind {
        ResourceKind::Image => {
            podman.remove_image(&candidate.name)?;
            if let Some(runtime) = candidate.runtime {
                remove_default_runtime_image_state(runtime)?;
            }
            Ok(())
        }
        ResourceKind::Volume => podman.remove_volume(&candidate.name),
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
