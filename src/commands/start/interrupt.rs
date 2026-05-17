// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::commands::container_cleanup::ManagedContainerCleanup;
use crate::commands::launch_policy::CommandInterrupt;
use crate::podman::Podman;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

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

    pub(super) fn check_interrupted(self, interrupt: &CommandInterrupt) -> Result<()> {
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
            "start interrupted before managed session `{}` for `{}` became ready",
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
            "start interrupted before managed session `agentbox-demo` for `/workspace/demo` became ready; cleaned up managed container `agentbox-demo` and preserved existing cache volume `agentbox-demo`; default runtime image was left untouched",
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
            "start interrupted before managed session `agentbox-demo` for `/workspace/demo` became ready; cleaned up managed container `agentbox-demo` and removed newly-created cache volume `agentbox-demo`; default runtime image was left untouched",
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
            "start interrupted before managed session `agentbox-demo` for `/workspace/demo` became ready; partial cleanup failed: container still exists after cleanup; cache volume removal failed: denied; default runtime image was left untouched",
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
