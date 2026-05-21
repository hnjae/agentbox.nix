// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::Args;

use crate::diagnostic;
use crate::podman::Podman;
use crate::prompt;
use crate::{Error, Result};

use super::runtime::remove_default_runtime_image_state_if_image;
use plan::{CleanCandidate, CleanInventory, CleanPlan, CleanResource, CleanScope, SkippedResource};

mod inventory;
mod plan;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct CleanArgs {
    /// Print cleanup candidates without deleting anything.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,

    /// Delete cleanup candidates without prompting.
    #[arg(long)]
    pub yes: bool,

    /// Consider unused default runtime images.
    #[arg(long)]
    pub images: bool,

    /// Consider unused workspace cache volumes.
    #[arg(long)]
    pub volumes: bool,
}

pub fn run(args: CleanArgs) -> Result<()> {
    let podman = Podman::new();
    let scope = CleanScope::from_flags(args.images, args.volumes);
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

fn build_clean_plan(podman: &Podman, scope: CleanScope) -> Result<CleanPlan> {
    let inventory = CleanInventory::from_podman(podman, &scope)?;

    Ok(CleanPlan::from_inventory(&scope, &inventory))
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
