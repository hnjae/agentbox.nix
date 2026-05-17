// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::runtime::RuntimeRunMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HostClientRequirement {
    Required,
    NotRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExistingResourceScope {
    ManagedSessions,
    AgentboxContainers,
}

impl ExistingResourceScope {
    pub(super) fn diagnostic_message(self) -> &'static str {
        match self {
            Self::ManagedSessions => "checking existing managed sessions",
            Self::AgentboxContainers => "checking existing agentbox containers",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExistingResourceCheck {
    RequireAbsent(ExistingResourceScope),
    AllowExisting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeLaunchPolicy {
    pub(super) run_mode: RuntimeRunMode,
    pub(super) host_client: HostClientRequirement,
    pub(super) existing_check: ExistingResourceCheck,
    pub(super) record_runtime_image_version: bool,
}

impl RuntimeLaunchPolicy {
    pub(super) fn managed_server(connect_after_start: bool) -> Self {
        Self {
            run_mode: RuntimeRunMode::ManagedSession,
            host_client: host_client_for_connect(connect_after_start),
            existing_check: ExistingResourceCheck::RequireAbsent(
                ExistingResourceScope::AgentboxContainers,
            ),
            record_runtime_image_version: true,
        }
    }

    pub(super) fn transient_server() -> Self {
        Self {
            run_mode: RuntimeRunMode::TransientServer,
            host_client: HostClientRequirement::Required,
            existing_check: ExistingResourceCheck::RequireAbsent(
                ExistingResourceScope::AgentboxContainers,
            ),
            record_runtime_image_version: false,
        }
    }

    pub(super) fn foreground() -> Self {
        Self {
            run_mode: RuntimeRunMode::Foreground,
            host_client: HostClientRequirement::NotRequired,
            existing_check: ExistingResourceCheck::RequireAbsent(
                ExistingResourceScope::ManagedSessions,
            ),
            record_runtime_image_version: false,
        }
    }

    pub(super) fn replacement_server(connect_after_start: bool) -> Self {
        Self {
            run_mode: RuntimeRunMode::ManagedSession,
            host_client: host_client_for_connect(connect_after_start),
            existing_check: ExistingResourceCheck::AllowExisting,
            record_runtime_image_version: true,
        }
    }
}

fn host_client_for_connect(connect_after_start: bool) -> HostClientRequirement {
    if connect_after_start {
        HostClientRequirement::Required
    } else {
        HostClientRequirement::NotRequired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_policies_select_runtime_run_mode() {
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(false).run_mode,
            RuntimeRunMode::ManagedSession
        );
        assert_eq!(
            RuntimeLaunchPolicy::replacement_server(false).run_mode,
            RuntimeRunMode::ManagedSession
        );
        assert_eq!(
            RuntimeLaunchPolicy::transient_server().run_mode,
            RuntimeRunMode::TransientServer
        );
        assert_eq!(
            RuntimeLaunchPolicy::foreground().run_mode,
            RuntimeRunMode::Foreground
        );
    }

    #[test]
    fn launch_policies_select_host_client_requirement() {
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(false).host_client,
            HostClientRequirement::NotRequired
        );
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(true).host_client,
            HostClientRequirement::Required
        );
        assert_eq!(
            RuntimeLaunchPolicy::transient_server().host_client,
            HostClientRequirement::Required
        );
        assert_eq!(
            RuntimeLaunchPolicy::foreground().host_client,
            HostClientRequirement::NotRequired
        );
        assert_eq!(
            RuntimeLaunchPolicy::replacement_server(true).host_client,
            HostClientRequirement::Required
        );
        assert_eq!(
            RuntimeLaunchPolicy::replacement_server(false).host_client,
            HostClientRequirement::NotRequired
        );
    }

    #[test]
    fn launch_policies_select_existing_resource_check() {
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(false).existing_check,
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
        );
        assert_eq!(
            RuntimeLaunchPolicy::managed_server(true).existing_check,
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
        );
        assert_eq!(
            RuntimeLaunchPolicy::transient_server().existing_check,
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::AgentboxContainers)
        );
        assert_eq!(
            RuntimeLaunchPolicy::foreground().existing_check,
            ExistingResourceCheck::RequireAbsent(ExistingResourceScope::ManagedSessions)
        );
        assert_eq!(
            RuntimeLaunchPolicy::replacement_server(false).existing_check,
            ExistingResourceCheck::AllowExisting
        );
    }

    #[test]
    fn launch_policies_record_runtime_image_versions_only_for_managed_lifetimes() {
        assert!(RuntimeLaunchPolicy::managed_server(false).record_runtime_image_version);
        assert!(RuntimeLaunchPolicy::managed_server(true).record_runtime_image_version);
        assert!(RuntimeLaunchPolicy::replacement_server(false).record_runtime_image_version);
        assert!(!RuntimeLaunchPolicy::transient_server().record_runtime_image_version);
        assert!(!RuntimeLaunchPolicy::foreground().record_runtime_image_version);
    }

    #[test]
    fn existing_resource_scopes_keep_diagnostic_messages_near_scope_policy() {
        assert_eq!(
            ExistingResourceScope::ManagedSessions.diagnostic_message(),
            "checking existing managed sessions"
        );
        assert_eq!(
            ExistingResourceScope::AgentboxContainers.diagnostic_message(),
            "checking existing agentbox containers"
        );
    }
}
