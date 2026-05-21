// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::{Duration, Instant};

use crate::podman::{Podman, PodmanContainerInspect};
use crate::runtime::{HostRuntimeHealthProbe, RuntimeHealthProbe, RuntimeKind};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

use super::endpoint::{ServerEndpointContext, ServerEndpointState, inspect_server_endpoint};
use super::{SERVER_READINESS_POLL_INTERVAL, SERVER_READINESS_TIMEOUT, ServerEndpointWait};

#[derive(Debug, Clone)]
pub(super) struct ServerEndpointWaiter<P> {
    probe: P,
    timeout: Duration,
    poll_interval: Duration,
}

impl ServerEndpointWaiter<HostRuntimeHealthProbe> {
    pub(super) fn production() -> Self {
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
    pub(super) fn wait(
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
                Ok(inspect) => match inspect_server_endpoint(
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
                },
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::commands::server_readiness::test_support::{endpoint, running_inspect, workspace};
    use crate::runtime::{AttachEndpoint, RuntimeHealth};

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
}
