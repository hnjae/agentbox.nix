// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::{Error, Result};

const AGENTBOX_STATE_DIR: &str = "agentbox";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentboxStateRoot {
    root: PathBuf,
}

impl AgentboxStateRoot {
    pub(crate) fn from_xdg() -> Result<Self> {
        let base_dirs = BaseDirs::new().ok_or_else(xdg_state_dir_error)?;
        let state_home = base_dirs.state_dir().ok_or_else(xdg_state_dir_error)?;

        Ok(Self::from_state_home(state_home))
    }

    pub(crate) fn from_state_home(state_home: impl AsRef<Path>) -> Self {
        Self {
            root: state_home.as_ref().join(AGENTBOX_STATE_DIR),
        }
    }

    pub(crate) fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.root.join(path)
    }
}

fn xdg_state_dir_error() -> Error {
    Error::msg("failed to resolve XDG state directory")
}
