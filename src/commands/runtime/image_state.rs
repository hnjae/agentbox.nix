// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::runtime::RuntimeKind;
use crate::state::AgentboxStateRoot;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct RuntimeImageState {
    runtime: String,
    package: String,
    install_source: String,
    pub(super) image: String,
    pub(super) image_context_hash: Option<String>,
    pub(super) installed_version: String,
    latest_seen_version: String,
    latest_checked_at: u64,
    image_built_at: u64,
}

impl RuntimeImageState {
    pub(super) fn new(
        runtime: RuntimeKind,
        version: String,
        latest_checked_at: u64,
        image_built_at: u64,
    ) -> Self {
        let package = runtime.package_spec();
        Self {
            runtime: runtime.as_str().to_string(),
            package: package.name.to_string(),
            install_source: package.install_source.to_string(),
            image: runtime.default_image().to_string(),
            image_context_hash: Some(runtime.default_image_context_hash().to_string()),
            installed_version: version.clone(),
            latest_seen_version: version,
            latest_checked_at,
            image_built_at,
        }
    }

    pub(super) fn with_latest_check(
        mut self,
        latest_version: String,
        latest_checked_at: u64,
    ) -> Self {
        self.latest_seen_version = latest_version;
        self.latest_checked_at = latest_checked_at;
        self
    }
}

pub(super) fn read_runtime_image_state(runtime: RuntimeKind) -> Result<Option<RuntimeImageState>> {
    let path = runtime_image_state_path(runtime)?;
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)?;
    serde_json::from_str(&contents).map(Some).map_err(|error| {
        Error::msg(format!(
            "failed to parse {runtime} runtime image state `{}`: {error}",
            path.display()
        ))
    })
}

pub(super) fn write_runtime_image_state(
    runtime: RuntimeKind,
    state: &RuntimeImageState,
) -> Result<()> {
    let path = runtime_image_state_path(runtime)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(state).map_err(|error| {
        Error::msg(format!(
            "failed to serialize {runtime} runtime image state: {error}"
        ))
    })?;
    fs::write(path, format!("{contents}\n"))?;
    Ok(())
}

pub(super) fn remove_runtime_image_state(runtime: RuntimeKind) -> Result<()> {
    let path = runtime_image_state_path(runtime)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn remove_runtime_image_state_if_image(runtime: RuntimeKind, image: &str) -> Result<()> {
    let Some(state) = read_runtime_image_state(runtime)? else {
        return Ok(());
    };

    if state.image == image {
        remove_runtime_image_state(runtime)?;
    }

    Ok(())
}

fn runtime_image_state_path(runtime: RuntimeKind) -> Result<PathBuf> {
    Ok(AgentboxStateRoot::from_xdg()?
        .join("runtime")
        .join(format!("{}.json", runtime.as_str())))
}
