// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::plan::{CleanPlan, SkippedResource};

pub(super) fn render_plan(plan: &CleanPlan) -> String {
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

#[cfg(test)]
mod tests {
    use crate::runtime::RuntimeKind;

    use super::*;
    use crate::commands::clean::plan::{CleanCandidate, SkippedResource};
    use crate::commands::clean::resource::CleanResource;

    #[test]
    fn render_plan_reports_empty_plans() {
        assert_eq!(render_plan(&CleanPlan::default()), "nothing to clean\n");
    }

    #[test]
    fn render_plan_groups_candidates_and_skipped_resources() {
        let plan = CleanPlan {
            candidates: vec![
                CleanCandidate::default_runtime_image(
                    RuntimeKind::Opencode,
                    CleanResource::image("localhost/agentbox-opencode:ctx-0123456789abcdef"),
                ),
                CleanCandidate::cache_volume(CleanResource::volume("agentbox-demo")),
            ],
            skipped: vec![SkippedResource {
                resource: CleanResource::volume("agentbox-mounted"),
                reason: "mounted by container `running-id`".to_string(),
            }],
        };

        assert_eq!(
            render_plan(&plan),
            concat!(
                "cleanup candidates:\n",
                "- image `localhost/agentbox-opencode:ctx-0123456789abcdef`\n",
                "- volume `agentbox-demo`\n",
                "skipped:\n",
                "- volume `agentbox-mounted`: mounted by container `running-id`\n",
            )
        );
    }
}
