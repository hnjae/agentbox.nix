// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::podman::PodmanContainerInspect;
use crate::runtime::{AttachEndpoint, RuntimeHealthProbe, RuntimeKind};
use crate::session::discover_attach_endpoint_from_inspect;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::commands) enum ServerEndpointContext {
    ManagedSession,
    TransientRunContainer,
}

impl ServerEndpointContext {
    pub(super) fn description(self) -> &'static str {
        match self {
            Self::ManagedSession => "managed session",
            Self::TransientRunContainer => "transient run container",
        }
    }
}

#[derive(Debug)]
pub(super) enum ServerEndpointState {
    Ready(AttachEndpoint),
    Pending(String),
}

pub(super) fn inspect_server_endpoint<P>(
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    context: ServerEndpointContext,
    inspect: PodmanContainerInspect,
    probe: &P,
) -> Result<ServerEndpointState>
where
    P: RuntimeHealthProbe,
{
    if !inspect.state.running {
        return Err(Error::msg(format!(
            "{} `{}` for `{}` exited before the `{}` runtime server became reachable; status: {}, exit code: {}",
            context.description(),
            workspace.container_name,
            workspace.canonical_git_root,
            runtime.as_str(),
            inspect.state.status,
            inspect.state.exit_code,
        )));
    }

    let endpoint = match context {
        ServerEndpointContext::ManagedSession => discover_attach_endpoint_from_inspect(&inspect),
        ServerEndpointContext::TransientRunContainer => {
            discover_attach_endpoint_from_runtime_inspect(runtime, &inspect)
        }
    };

    match endpoint {
        Ok(endpoint) => {
            let health = probe.check(runtime, &endpoint);
            if health.is_healthy() {
                Ok(ServerEndpointState::Ready(endpoint))
            } else {
                tracing::debug!(
                    endpoint = %endpoint,
                    reason = health.reason(),
                    "runtime endpoint probe is not ready"
                );
                Ok(ServerEndpointState::Pending(format!(
                    "endpoint `{endpoint}` is not reachable yet"
                )))
            }
        }
        Err(error) => Ok(ServerEndpointState::Pending(error.to_string())),
    }
}

fn discover_attach_endpoint_from_runtime_inspect(
    runtime: RuntimeKind,
    inspect: &PodmanContainerInspect,
) -> Result<AttachEndpoint> {
    let attach = runtime.attach_spec();
    let port_key = attach.tcp_port_key();
    let published_port = inspect
        .network_settings
        .published_attach_endpoint(attach)?
        .ok_or_else(|| {
            Error::msg(format!(
                "transient run container has no published attach port for `{port_key}`"
            ))
        })?;

    Ok(published_port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::server_readiness::test_support::{endpoint, running_inspect, workspace};
    use crate::runtime::RuntimeHealth;

    #[test]
    fn managed_session_endpoint_uses_session_attach_labels() {
        let workspace = workspace();
        let endpoint = endpoint();

        let state = inspect_server_endpoint(
            &workspace,
            RuntimeKind::Opencode,
            ServerEndpointContext::ManagedSession,
            running_inspect(&workspace, Some(endpoint.host_port)),
            &StaticProbe {
                health: RuntimeHealth::Healthy,
            },
        )
        .unwrap();

        assert!(matches!(state, ServerEndpointState::Ready(actual) if actual == endpoint));
    }

    #[test]
    fn unhealthy_probe_keeps_endpoint_pending() {
        let workspace = workspace();

        let state = inspect_server_endpoint(
            &workspace,
            RuntimeKind::Opencode,
            ServerEndpointContext::ManagedSession,
            running_inspect(&workspace, Some(endpoint().host_port)),
            &StaticProbe {
                health: RuntimeHealth::Unhealthy {
                    reason: "booting".to_string(),
                },
            },
        )
        .unwrap();

        assert!(matches!(
            state,
            ServerEndpointState::Pending(reason)
                if reason == "endpoint `http://127.0.0.1:49152` is not reachable yet"
        ));
    }

    #[test]
    fn transient_run_endpoint_uses_runtime_attach_port() {
        let workspace = workspace();
        let endpoint = endpoint();

        let state = inspect_server_endpoint(
            &workspace,
            RuntimeKind::Opencode,
            ServerEndpointContext::TransientRunContainer,
            running_inspect(&workspace, Some(endpoint.host_port)),
            &StaticProbe {
                health: RuntimeHealth::Healthy,
            },
        )
        .unwrap();

        assert!(matches!(state, ServerEndpointState::Ready(actual) if actual == endpoint));
    }

    #[test]
    fn exited_container_fails_readiness_immediately() {
        let workspace = workspace();
        let mut inspect = running_inspect(&workspace, Some(endpoint().host_port));
        inspect.state.running = false;
        inspect.state.status = "exited".to_string();
        inspect.state.exit_code = 42;

        let error = inspect_server_endpoint(
            &workspace,
            RuntimeKind::Opencode,
            ServerEndpointContext::ManagedSession,
            inspect,
            &StaticProbe {
                health: RuntimeHealth::Healthy,
            },
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "managed session `agentbox-demo` for `/workspace/demo` exited before the `opencode` runtime server became reachable; status: exited, exit code: 42"
        );
    }

    #[derive(Debug, Clone)]
    struct StaticProbe {
        health: RuntimeHealth,
    }

    impl RuntimeHealthProbe for StaticProbe {
        fn check(&self, _runtime: RuntimeKind, _endpoint: &AttachEndpoint) -> RuntimeHealth {
            self.health.clone()
        }
    }
}
