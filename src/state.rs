// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

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
