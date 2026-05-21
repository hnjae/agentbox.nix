// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::{Duration, Instant};

use crate::podman::{Podman, PodmanContainerInspect};
use crate::runtime::{AttachEndpoint, HostRuntimeHealthProbe, RuntimeHealthProbe, RuntimeKind};
use crate::session::discover_attach_endpoint_from_inspect;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

const SERVER_READINESS_TIMEOUT: Duration = Duration::from_secs(30);
const SERVER_READINESS_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub(super) fn wait_for_server_endpoint(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    runtime: RuntimeKind,
    context: ServerEndpointContext,
    interrupted: impl Fn() -> bool,
) -> Result<ServerEndpointWait> {
    ServerEndpointWaiter::production().wait(podman, workspace, runtime, context, interrupted)
}

#[derive(Debug, Clone)]
struct ServerEndpointWaiter<P> {
    probe: P,
    timeout: Duration,
    poll_interval: Duration,
}

impl ServerEndpointWaiter<HostRuntimeHealthProbe> {
    fn production() -> Self {
        Self {
            probe: HostRuntimeHealthProbe,
            timeout: SERVER_READINESS_TIMEOUT,
            poll_interval: SERVER_READINESS_POLL_INTERVAL,
        }
    }
}

impl<P> ServerEndpointWaiter<P>
where
    P: RuntimeHealthProbe,
{
    fn wait(
        &self,
        podman: &Podman,
        workspace: &WorkspaceIdentity,
        runtime: RuntimeKind,
        context: ServerEndpointContext,
        interrupted: impl Fn() -> bool,
    ) -> Result<ServerEndpointWait> {
        let mut clock = SystemReadinessClock;
        self.wait_with(
            workspace,
            runtime,
            context,
            interrupted,
            |container_name| podman.inspect_one(container_name),
            &mut clock,
        )
    }

    fn wait_with(
        &self,
        workspace: &WorkspaceIdentity,
        runtime: RuntimeKind,
        context: ServerEndpointContext,
        interrupted: impl Fn() -> bool,
        mut inspect_container: impl FnMut(&str) -> Result<PodmanContainerInspect>,
        clock: &mut impl ReadinessClock,
    ) -> Result<ServerEndpointWait> {
        let deadline = clock.now() + self.timeout;
        let mut last_error = None::<String>;

        loop {
            if interrupted() {
                return Ok(ServerEndpointWait::Interrupted);
            }

            if clock.now() >= deadline {
                let last_error = last_error
                    .as_deref()
                    .unwrap_or("no inspect data was available");
                return Err(Error::msg(format!(
                    "runtime server for {} `{}` in `{}` did not become reachable: {last_error}",
                    context.description(),
                    workspace.container_name,
                    workspace.canonical_git_root,
                )));
            }

            match inspect_container(&workspace.container_name) {
                Ok(inspect) => {
                    match inspect_server_endpoint(
                        workspace,
                        runtime,
                        context,
                        inspect,
                        &self.probe,
                    )? {
                        ServerEndpointState::Ready(endpoint) => {
                            return Ok(ServerEndpointWait::Ready(endpoint));
                        }
                        ServerEndpointState::Pending(error) => last_error = Some(error),
                    }
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                }
            }

            clock.sleep(self.poll_interval);
        }
    }
}

trait ReadinessClock {
    fn now(&mut self) -> Instant;

    fn sleep(&mut self, duration: Duration);
}

struct SystemReadinessClock;

impl ReadinessClock for SystemReadinessClock {
    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

#[derive(Debug)]
pub(super) enum ServerEndpointWait {
    Ready(AttachEndpoint),
    Interrupted,
}

enum ServerEndpointState {
    Ready(AttachEndpoint),
    Pending(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ServerEndpointContext {
    ManagedSession,
    TransientRunContainer,
}

impl ServerEndpointContext {
    fn description(self) -> &'static str {
        match self {
            Self::ManagedSession => "managed session",
            Self::TransientRunContainer => "transient run container",
        }
    }
}

fn inspect_server_endpoint<P>(
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
    use std::collections::{BTreeMap, VecDeque};

    use camino::Utf8PathBuf;

    use super::*;
    use crate::metadata::{ManagedSessionLabelInput, managed_session_labels};
    use crate::podman::{
        PodmanContainerConfig, PodmanContainerState, PodmanHostConfig, PodmanNetworkSettings,
        PodmanPortBinding,
    };
    use crate::runtime::RuntimeHealth;

    #[test]
    fn readiness_waiter_retries_until_inspect_endpoint_is_ready() {
        let workspace = workspace();
        let endpoint = endpoint();
        let waiter = test_waiter(RuntimeHealth::Healthy);
        let mut clock = FakeClock::new();
        let mut inspects = VecDeque::from([
            running_inspect(&workspace, None),
            running_inspect(&workspace, Some(endpoint.host_port)),
        ]);

        let result = waiter
            .wait_with(
                &workspace,
                RuntimeKind::Opencode,
                ServerEndpointContext::ManagedSession,
                || false,
                |container_name| {
                    assert_eq!(container_name, workspace.container_name);
                    Ok(inspects.pop_front().expect("expected inspect fixture"))
                },
                &mut clock,
            )
            .unwrap();

        match result {
            ServerEndpointWait::Ready(actual) => assert_eq!(actual, endpoint),
            ServerEndpointWait::Interrupted => panic!("readiness should not be interrupted"),
        }
        assert!(inspects.is_empty());
        assert_eq!(clock.sleeps, vec![Duration::from_millis(10)]);
    }

    #[test]
    fn readiness_waiter_times_out_with_last_inspect_error() {
        let workspace = workspace();
        let waiter = test_waiter(RuntimeHealth::Healthy);
        let mut clock = FakeClock::new();
        let mut inspect_calls = 0;

        let error = waiter
            .wait_with(
                &workspace,
                RuntimeKind::Opencode,
                ServerEndpointContext::ManagedSession,
                || false,
                |_| {
                    inspect_calls += 1;
                    Err(Error::msg("inspect unavailable"))
                },
                &mut clock,
            )
            .unwrap_err();

        assert_eq!(inspect_calls, 3);
        assert_eq!(
            error.to_string(),
            "runtime server for managed session `agentbox-demo` in `/workspace/demo` did not become reachable: inspect unavailable"
        );
        assert_eq!(
            clock.sleeps,
            vec![
                Duration::from_millis(10),
                Duration::from_millis(10),
                Duration::from_millis(10),
            ]
        );
    }

    #[test]
    fn readiness_waiter_stops_before_inspect_when_interrupted() {
        let workspace = workspace();
        let waiter = test_waiter(RuntimeHealth::Healthy);
        let mut clock = FakeClock::new();

        let result = waiter
            .wait_with(
                &workspace,
                RuntimeKind::Opencode,
                ServerEndpointContext::ManagedSession,
                || true,
                |_| panic!("interrupted wait must not inspect"),
                &mut clock,
            )
            .unwrap();

        assert!(matches!(result, ServerEndpointWait::Interrupted));
        assert!(clock.sleeps.is_empty());
    }

    fn test_waiter(health: RuntimeHealth) -> ServerEndpointWaiter<StaticProbe> {
        ServerEndpointWaiter {
            probe: StaticProbe { health },
            timeout: Duration::from_millis(25),
            poll_interval: Duration::from_millis(10),
        }
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

    struct FakeClock {
        now: Instant,
        sleeps: Vec<Duration>,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                now: Instant::now(),
                sleeps: Vec::new(),
            }
        }
    }

    impl ReadinessClock for FakeClock {
        fn now(&mut self) -> Instant {
            self.now
        }

        fn sleep(&mut self, duration: Duration) {
            self.sleeps.push(duration);
            self.now += duration;
        }
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

    fn endpoint() -> AttachEndpoint {
        AttachEndpoint {
            scheme: "http".to_string(),
            host_ip: "127.0.0.1".to_string(),
            host_port: 49152,
        }
    }

    fn running_inspect(
        workspace: &WorkspaceIdentity,
        host_port: Option<u16>,
    ) -> PodmanContainerInspect {
        let runtime = RuntimeKind::Opencode;
        let image = runtime.default_image();
        let labels = managed_session_labels(ManagedSessionLabelInput {
            canonical_git_root: workspace.canonical_git_root.as_str(),
            git_root_hash: workspace.hash12.as_str(),
            runtime,
            image: &image,
            launch_directory: workspace.canonical_target.as_str(),
            logical_name: workspace.container_name.as_str(),
        });

        PodmanContainerInspect {
            id: "container-id".to_string(),
            path: "/usr/bin/opencode".to_string(),
            state: PodmanContainerState {
                status: "running".to_string(),
                running: true,
                pid: 4321,
                ..PodmanContainerState::default()
            },
            image_name: image,
            config: PodmanContainerConfig {
                labels,
                ..PodmanContainerConfig::default()
            },
            host_config: PodmanHostConfig {
                network_mode: Some("bridge".to_string()),
                ..PodmanHostConfig::default()
            },
            network_settings: network_settings(host_port),
            ..PodmanContainerInspect::default()
        }
    }

    fn network_settings(host_port: Option<u16>) -> PodmanNetworkSettings {
        let ports = host_port
            .map(|host_port| {
                BTreeMap::from([(
                    "4096/tcp".to_string(),
                    Some(vec![PodmanPortBinding {
                        host_ip: Some("127.0.0.1".to_string()),
                        host_port: Some(host_port.to_string()),
                    }]),
                )])
            })
            .unwrap_or_default();

        PodmanNetworkSettings {
            ports,
            ..PodmanNetworkSettings::default()
        }
    }
}
