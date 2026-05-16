// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

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
