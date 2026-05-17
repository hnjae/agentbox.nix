// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::{Args, Subcommand};

use crate::Result;
use crate::runtime::RuntimeKind;

mod image_lifecycle;
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
pub struct RuntimeUpdateArgs {
    /// Runtime image to update.
    #[arg(value_enum)]
    pub runtime: RuntimeKind,
}

pub fn run(args: RuntimeArgs, verbose: bool) -> Result<()> {
    match args.command {
        RuntimeCommand::Update(args) => {
            image_lifecycle::update_default_runtime_image(args.runtime, verbose)
        }
    }
}
