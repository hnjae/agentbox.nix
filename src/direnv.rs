// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::Utf8Path;
use serde::Deserialize;

use crate::process::ProcessRunner;
use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Direnv {
    runner: ProcessRunner,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct DirenvEnvironment {
    pub entries: BTreeMap<String, Option<String>>,
}

impl Direnv {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_runner(runner: ProcessRunner) -> Self {
        Self { runner }
    }

    pub fn export_json(&self, directory: &Utf8Path) -> Result<DirenvEnvironment> {
        let output = self.runner.capture("direnv", |command| {
            command
                .current_dir(directory.as_std_path())
                .args(["export", "json"]);
        })?;

        serde_json::from_str(&output.stdout)
            .map_err(|error| Error::msg(format!("failed to parse `direnv export json`: {error}")))
    }
}
