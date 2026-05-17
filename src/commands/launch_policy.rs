// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::podman::Podman;
use crate::prompt;
use crate::runtime::RuntimeKind;
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

const FAILURE_LOG_TAIL_LINES: usize = 80;

pub(super) fn select_runtime(
    runtime: Option<RuntimeKind>,
    non_tty_error: &'static str,
) -> Result<RuntimeKind> {
    match runtime {
        Some(runtime) => Ok(runtime),
        None => prompt::select_one(
            "Select runtime",
            RuntimeKind::variants().to_vec(),
            non_tty_error,
        ),
    }
}

pub(super) fn exit_code(code: i32) -> Option<u8> {
    u8::try_from(code).ok()
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ContainerLogContext {
    ManagedSession,
    TransientRun,
}

impl ContainerLogContext {
    fn description(self) -> &'static str {
        match self {
            Self::ManagedSession => "container",
            Self::TransientRun => "transient run container",
        }
    }
}

pub(super) fn error_with_container_logs(
    podman: &Podman,
    workspace: &WorkspaceIdentity,
    context: ContainerLogContext,
    original_error: Error,
) -> Error {
    let container_name = &workspace.container_name;
    let description = context.description();
    let command = format!("podman logs --tail {FAILURE_LOG_TAIL_LINES} {container_name}");

    match podman.logs_tail(container_name, FAILURE_LOG_TAIL_LINES) {
        Ok(logs) => {
            let logs = logs.trim_end();
            if logs.is_empty() {
                Error::msg(format!(
                    "{original_error}\n\n{description} `{container_name}` produced no logs; inspect it with `{command}`"
                ))
            } else {
                Error::msg(format!(
                    "{original_error}\n\n{description} logs (`{command}`):\n{logs}"
                ))
            }
        }
        Err(log_error) => Error::msg(format!(
            "{original_error}\n\nfailed to read {description} logs with `{command}`: {log_error}"
        )),
    }
}

#[derive(Debug)]
pub(super) struct CommandInterrupt {
    flag: Arc<AtomicBool>,
    signal_id: Option<signal_hook::SigId>,
}

impl CommandInterrupt {
    pub(super) fn install(command_name: &'static str) -> Result<Self> {
        let flag = Arc::new(AtomicBool::new(false));
        let signal_id = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&flag))
            .map_err(|error| {
                Error::msg(format!(
                    "failed to install SIGINT cleanup handler for `agentbox {command_name}`: {error}"
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

impl Drop for CommandInterrupt {
    fn drop(&mut self) {
        if let Some(signal_id) = self.signal_id.take() {
            signal_hook::low_level::unregister(signal_id);
        }
    }
}
