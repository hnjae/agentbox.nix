// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::{Utf8Path, Utf8PathBuf};

use crate::process::ProcessRunner;
use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Git {
    runner: ProcessRunner,
}

impl Git {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_runner(runner: ProcessRunner) -> Self {
        Self { runner }
    }

    pub fn rev_parse_show_toplevel(&self, directory: &Utf8Path) -> Result<Utf8PathBuf> {
        let output = self.runner.capture("git", |command| {
            command
                .arg("-C")
                .arg(directory.as_str())
                .args(["rev-parse", "--show-toplevel"]);
        })?;

        let root = output.stdout.trim();
        if root.is_empty() {
            return Err(Error::msg(
                "`git rev-parse --show-toplevel` returned an empty path",
            ));
        }

        Ok(Utf8PathBuf::from(root.to_owned()))
    }
}
