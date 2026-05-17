// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Result;
use crate::cli::{RuntimeArgs, RuntimeCommand};

mod image_lifecycle;
mod image_state;

pub(super) use image_lifecycle::{
    ensure_default_runtime_image, remove_default_runtime_image_state_if_image,
};

pub fn run(args: RuntimeArgs, verbose: bool) -> Result<()> {
    match args.command {
        RuntimeCommand::Update(args) => {
            image_lifecycle::update_default_runtime_image(args.runtime, verbose)
        }
    }
}
