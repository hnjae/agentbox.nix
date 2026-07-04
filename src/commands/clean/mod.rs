// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::Args;

use crate::Result;
use crate::podman::Podman;
use crate::prompt;

use apply::apply_clean_plan;
use inventory::CleanInventory;
use plan::CleanPlan;
use render::render_plan;
use scope::CleanScope;

mod apply;
mod inventory;
mod plan;
mod render;
mod resource;
mod scope;
mod usage;

#[derive(Debug, Args, PartialEq, Eq)]
pub struct CleanArgs {
    /// Print cleanup candidates without deleting anything.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,

    /// Delete cleanup candidates without prompting.
    #[arg(long)]
    pub yes: bool,

    /// Limit cleanup to unused default runtime images.
    #[arg(long)]
    pub images: bool,

    /// Limit cleanup to unused workspace cache volumes.
    #[arg(long)]
    pub volumes: bool,

    /// Limit cleanup to unused workspace lock files.
    #[arg(long)]
    pub locks: bool,
}

pub fn run(args: CleanArgs) -> Result<()> {
    let podman = Podman::new();
    let scope = CleanScope::from_flags(args.images, args.volumes, args.locks);
    let plan = build_clean_plan(&podman, scope)?;

    if args.dry_run {
        crate::diagnostic::info(render_plan(&plan));
        return Ok(());
    }

    if plan.candidates.is_empty() && plan.skipped.is_empty() {
        crate::diagnostic::info("nothing to clean");
        return Ok(());
    }

    crate::diagnostic::info(render_plan(&plan));
    if plan.candidates.is_empty() {
        return Ok(());
    }

    if !args.yes && !confirm_interactive()? {
        crate::diagnostic::warning("aborted");
        return Ok(());
    }

    apply_clean_plan(&podman, &plan)
}

fn build_clean_plan(podman: &Podman, scope: CleanScope) -> Result<CleanPlan> {
    let inventory = CleanInventory::from_podman(podman, &scope)?;

    Ok(CleanPlan::from_inventory(&scope, &inventory))
}

fn confirm_interactive() -> Result<bool> {
    prompt::confirm(
        "Proceed?",
        false,
        "agentbox clean requires --yes or --dry-run when stdin or stderr is not a TTY",
    )
}
