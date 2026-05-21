// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::podman::Podman;
use crate::{Error, Result};

use super::plan::{CleanCandidate, CleanPlan};
use super::resource::CleanResource;

pub(super) fn apply_clean_plan(podman: &Podman, plan: &CleanPlan) -> Result<()> {
    let mut removal = PodmanCleanRemoval { podman };
    apply_clean_plan_with(&mut removal, plan, crate::diagnostic::info)
}

fn apply_clean_plan_with(
    removal: &mut impl CleanRemoval,
    plan: &CleanPlan,
    mut report_removed: impl FnMut(String),
) -> Result<()> {
    let mut failures = Vec::new();

    for candidate in &plan.candidates {
        match removal.remove_candidate(candidate) {
            Ok(()) => report_removed(format!(
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

trait CleanRemoval {
    fn remove_candidate(&mut self, candidate: &CleanCandidate) -> Result<()>;
}

struct PodmanCleanRemoval<'a> {
    podman: &'a Podman,
}

impl CleanRemoval for PodmanCleanRemoval<'_> {
    fn remove_candidate(&mut self, candidate: &CleanCandidate) -> Result<()> {
        match candidate {
            CleanCandidate::DefaultRuntimeImage { runtime, resource } => {
                self.podman.remove_image(resource.name())?;
                super::super::runtime::remove_default_runtime_image_state_if_image(
                    *runtime,
                    resource.name(),
                )?;
                Ok(())
            }
            CleanCandidate::CacheVolume { resource } => self.podman.remove_volume(resource.name()),
        }
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
    use crate::runtime::RuntimeKind;

    use super::*;

    #[test]
    fn apply_clean_plan_continues_after_delete_failures() {
        let mut removal = RecordingRemoval {
            failures: ["localhost/agentbox-opencode:ctx-0123456789abcdef"],
            removed: Vec::new(),
        };
        let plan = CleanPlan {
            candidates: vec![
                CleanCandidate::default_runtime_image(
                    RuntimeKind::Opencode,
                    CleanResource::image("localhost/agentbox-opencode:ctx-0123456789abcdef"),
                ),
                CleanCandidate::cache_volume(CleanResource::volume("agentbox-demo")),
            ],
            skipped: Vec::new(),
        };
        let mut reports = Vec::new();

        let error = apply_clean_plan_with(&mut removal, &plan, |message| reports.push(message))
            .unwrap_err();

        assert_eq!(
            removal.removed,
            [
                "image:localhost/agentbox-opencode:ctx-0123456789abcdef",
                "volume:agentbox-demo",
            ]
        );
        assert_eq!(reports, ["removed volume `agentbox-demo`"]);
        assert_eq!(
            error.to_string(),
            "partial clean failed; failed resources: image `localhost/agentbox-opencode:ctx-0123456789abcdef` (delete failed)"
        );
    }

    struct RecordingRemoval<const N: usize> {
        failures: [&'static str; N],
        removed: Vec<String>,
    }

    impl<const N: usize> CleanRemoval for RecordingRemoval<N> {
        fn remove_candidate(&mut self, candidate: &CleanCandidate) -> Result<()> {
            self.removed.push(format!(
                "{}:{}",
                candidate.kind().as_str(),
                candidate.name()
            ));

            if self.failures.iter().any(|name| *name == candidate.name()) {
                Err(Error::msg("delete failed"))
            } else {
                Ok(())
            }
        }
    }
}
