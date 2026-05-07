// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::commands::container_cleanup::ManagedContainerCleanup;
use crate::podman::Podman;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

#[derive(Debug)]
pub(super) struct RunInterrupt {
    flag: Arc<AtomicBool>,
    signal_id: Option<signal_hook::SigId>,
}

impl RunInterrupt {
    pub(super) fn install() -> Result<Self> {
        let flag = Arc::new(AtomicBool::new(false));
        let signal_id = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&flag))
            .map_err(|error| {
                Error::msg(format!(
                    "failed to install SIGINT cleanup handler for `agentbox run`: {error}"
                ))
            })?;

        Ok(Self {
            flag,
            signal_id: Some(signal_id),
        })
    }

    pub(super) fn interrupted(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }
}

impl Drop for RunInterrupt {
    fn drop(&mut self) {
        if let Some(signal_id) = self.signal_id.take() {
            signal_hook::low_level::unregister(signal_id);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct InterruptedRunCleanupScope<'a> {
    podman: &'a Podman,
    workspace: &'a WorkspaceIdentity,
    cache_volume_existed_before: bool,
}

impl<'a> InterruptedRunCleanupScope<'a> {
    pub(super) fn new(
        podman: &'a Podman,
        workspace: &'a WorkspaceIdentity,
        cache_volume_existed_before: bool,
    ) -> Self {
        Self {
            podman,
            workspace,
            cache_volume_existed_before,
        }
    }

    pub(super) fn check_interrupted(self, interrupt: &RunInterrupt) -> Result<()> {
        if interrupt.interrupted() {
            Err(self.interrupted_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn interrupted_error(self) -> Error {
        let cleanup = InterruptedRunCleanup::run(
            self.podman,
            self.workspace,
            self.cache_volume_existed_before,
        );
        Error::msg(cleanup.render(self.workspace, self.cache_volume_existed_before))
    }
}

#[derive(Debug, Default)]
struct InterruptedRunCleanup {
    failures: Vec<String>,
    cache_volume_removed: bool,
}

impl InterruptedRunCleanup {
    fn run(
        podman: &Podman,
        workspace: &WorkspaceIdentity,
        cache_volume_existed_before: bool,
    ) -> Self {
        let mut cleanup = Self::default();
        let container_name = &workspace.container_name;
        let container_cleanup = ManagedContainerCleanup::stop_and_verify(podman, container_name);

        cleanup
            .failures
            .extend(container_cleanup.interrupted_messages());

        if container_cleanup.container_removed() {
            if !cache_volume_existed_before {
                match podman.remove_volume(container_name) {
                    Ok(()) => cleanup.cache_volume_removed = true,
                    Err(error) => cleanup
                        .failures
                        .push(format!("cache volume removal failed: {error}")),
                }
            }
        } else if !cache_volume_existed_before {
            if let Some(message) = container_cleanup.interrupted_cache_volume_skip_message() {
                cleanup.failures.push(message.to_string());
            }
        }

        cleanup
    }

    fn render(&self, workspace: &WorkspaceIdentity, cache_volume_existed_before: bool) -> String {
        let mut message = format!(
            "run interrupted before managed session `{}` for `{}` became ready",
            workspace.container_name, workspace.canonical_git_root,
        );

        if self.failures.is_empty() {
            let volume_detail = if cache_volume_existed_before {
                format!(
                    "preserved existing cache volume `{}`",
                    workspace.container_name
                )
            } else if self.cache_volume_removed {
                format!(
                    "removed newly-created cache volume `{}`",
                    workspace.container_name
                )
            } else {
                format!(
                    "no new cache volume `{}` remained",
                    workspace.container_name
                )
            };

            message.push_str(&format!(
                "; cleaned up managed container `{}` and {volume_detail}; default runtime image was left untouched",
                workspace.container_name,
            ));
        } else {
            message.push_str(&format!(
                "; partial cleanup failed: {}; default runtime image was left untouched",
                self.failures.join("; "),
            ));
        }

        message
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;

    #[test]
    fn cleanup_message_preserves_preexisting_cache_volume() {
        let workspace = workspace();
        let cleanup = InterruptedRunCleanup::default();

        assert_eq!(
            cleanup.render(&workspace, true),
            "run interrupted before managed session `agentbox-demo` for `/workspace/demo` became ready; cleaned up managed container `agentbox-demo` and preserved existing cache volume `agentbox-demo`; default runtime image was left untouched",
        );
    }

    #[test]
    fn cleanup_message_reports_removed_new_cache_volume() {
        let workspace = workspace();
        let cleanup = InterruptedRunCleanup {
            cache_volume_removed: true,
            ..InterruptedRunCleanup::default()
        };

        assert_eq!(
            cleanup.render(&workspace, false),
            "run interrupted before managed session `agentbox-demo` for `/workspace/demo` became ready; cleaned up managed container `agentbox-demo` and removed newly-created cache volume `agentbox-demo`; default runtime image was left untouched",
        );
    }

    #[test]
    fn cleanup_message_reports_partial_failures_without_volume_success_detail() {
        let workspace = workspace();
        let cleanup = InterruptedRunCleanup {
            failures: vec![
                "container still exists after cleanup".to_string(),
                "cache volume removal failed: denied".to_string(),
            ],
            cache_volume_removed: false,
        };

        assert_eq!(
            cleanup.render(&workspace, false),
            "run interrupted before managed session `agentbox-demo` for `/workspace/demo` became ready; partial cleanup failed: container still exists after cleanup; cache volume removal failed: denied; default runtime image was left untouched",
        );
    }

    fn workspace() -> WorkspaceIdentity {
        WorkspaceIdentity {
            requested_target: Utf8PathBuf::from("/workspace/demo"),
            absolute_target: Utf8PathBuf::from("/workspace/demo"),
            canonical_target: Utf8PathBuf::from("/workspace/demo"),
            canonical_git_root: Utf8PathBuf::from("/workspace/demo"),
            digest64: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            hash12: "0123456789ab".to_string(),
            container_name: "agentbox-demo".to_string(),
        }
    }
}
