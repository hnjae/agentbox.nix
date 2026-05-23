// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

mod command;
pub mod default_image;
mod health;
mod host_state;
mod http_probe;
mod kind;
mod profile;
mod spec;

pub const CODEX_REMOTE_TOKEN_ENV: &str = "AGENTBOX_CODEX_REMOTE_TOKEN";

pub(crate) use health::{HostRuntimeHealthProbe, RuntimeHealth, RuntimeHealthProbe};
pub(crate) use host_state::{RuntimeHostStateMount, RuntimeHostStateSource};
pub use kind::RuntimeKind;
pub use spec::{
    AttachEndpoint, DEFAULT_HOST_ATTACH_IP, RuntimeAttachSpec, RuntimeCommand, RuntimeCreateSpec,
    RuntimeInvocation, RuntimeMount, RuntimeMountKind, RuntimeRunMode, RuntimeRunSpec,
};
