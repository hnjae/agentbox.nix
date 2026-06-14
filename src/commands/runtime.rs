// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::{ArgGroup, Args, Subcommand};

use crate::Result;
use crate::runtime::RuntimeKind;

mod image_environment;
mod image_lifecycle;
mod image_plan;
mod image_state;

pub(super) use image_lifecycle::{
    ensure_default_runtime_image, remove_default_runtime_image_state_if_image,
};

#[derive(Debug, Args, PartialEq, Eq)]
pub struct RuntimeArgs {
    #[command(subcommand)]
    pub command: RuntimeCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum RuntimeCommand {
    /// Update a default runtime image.
    Update(RuntimeUpdateArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(group(
    ArgGroup::new("target")
        .required(true)
        .args(["runtime", "all"])
))]
pub struct RuntimeUpdateArgs {
    /// Runtime image to update.
    #[arg(value_enum)]
    pub runtime: Option<RuntimeKind>,

    /// Update every supported runtime image.
    #[arg(long, short = 'a')]
    pub all: bool,
}

pub fn run(args: RuntimeArgs, verbose: bool) -> Result<()> {
    match args.command {
        RuntimeCommand::Update(args) => update(args, verbose),
    }
}

fn update(args: RuntimeUpdateArgs, verbose: bool) -> Result<()> {
    match (args.runtime, args.all) {
        (Some(runtime), false) => image_lifecycle::update_default_runtime_image(runtime, verbose),
        (None, true) => {
            for runtime in RuntimeKind::variants() {
                image_lifecycle::update_default_runtime_image(*runtime, verbose)?;
            }

            Ok(())
        }
        _ => unreachable!("clap requires exactly one runtime update target"),
    }
}
