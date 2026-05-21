// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::runtime::RuntimeKind;
use crate::runtime::default_image::default_image_context_hash;
use crate::state::AgentboxStateRoot;
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct RuntimeImageState {
    runtime: String,
    package: String,
    install_source: String,
    pub(super) image: String,
    pub(super) image_context_hash: String,
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
            image: runtime.default_image(),
            image_context_hash: default_image_context_hash().to_string(),
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

#[derive(Debug, Clone)]
pub(super) struct RuntimeImageStateStore {
    root: AgentboxStateRoot,
}

impl RuntimeImageStateStore {
    pub(super) fn from_xdg() -> Result<Self> {
        Ok(Self::new(AgentboxStateRoot::from_xdg()?))
    }

    pub(super) fn new(root: AgentboxStateRoot) -> Self {
        Self { root }
    }

    pub(super) fn read(&self, runtime: RuntimeKind) -> Result<Option<RuntimeImageState>> {
        let path = self.path(runtime);
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

    pub(super) fn write(&self, runtime: RuntimeKind, state: &RuntimeImageState) -> Result<()> {
        let path = self.path(runtime);
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

    pub(super) fn remove(&self, runtime: RuntimeKind) -> Result<()> {
        let path = self.path(runtime);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn remove_if_image(&self, runtime: RuntimeKind, image: &str) -> Result<()> {
        let Some(state) = self.read(runtime)? else {
            return Ok(());
        };

        if state.image == image {
            self.remove(runtime)?;
        }

        Ok(())
    }

    fn path(&self, runtime: RuntimeKind) -> PathBuf {
        self.root
            .join("runtime")
            .join(format!("{}.json", runtime.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_store_round_trips_runtime_state_under_supplied_root() {
        let sandbox = tempfile::tempdir().unwrap();
        let store = RuntimeImageStateStore::new(AgentboxStateRoot::from_state_home(sandbox.path()));
        let runtime = RuntimeKind::Codex;
        let state = RuntimeImageState::new(runtime, "1.2.3".to_string(), 10, 11);

        assert_eq!(store.read(runtime).unwrap(), None);

        store.write(runtime, &state).unwrap();

        assert_eq!(store.read(runtime).unwrap(), Some(state));
        assert!(
            sandbox
                .path()
                .join("agentbox")
                .join("runtime")
                .join("codex.json")
                .is_file()
        );
    }

    #[test]
    fn state_store_removes_only_matching_image_state() {
        let sandbox = tempfile::tempdir().unwrap();
        let store = RuntimeImageStateStore::new(AgentboxStateRoot::from_state_home(sandbox.path()));
        let runtime = RuntimeKind::Opencode;
        let state = RuntimeImageState::new(runtime, "2.0.0".to_string(), 20, 21);

        store.write(runtime, &state).unwrap();
        store
            .remove_if_image(runtime, "localhost/agentbox-other:latest")
            .unwrap();

        assert_eq!(store.read(runtime).unwrap(), Some(state.clone()));

        store.remove_if_image(runtime, &state.image).unwrap();

        assert_eq!(store.read(runtime).unwrap(), None);
    }

    #[test]
    fn state_store_remove_ignores_missing_state_file() {
        let sandbox = tempfile::tempdir().unwrap();
        let store = RuntimeImageStateStore::new(AgentboxStateRoot::from_state_home(sandbox.path()));

        store.remove(RuntimeKind::Codex).unwrap();
    }
}
