// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) const CONTAINER_SSH_AUTH_SOCK: &str = "/run/agentbox/ssh-agent.sock";

mod agent_socket;
mod git_config;
mod known_hosts;
mod passthrough;
mod signing_key;

pub(crate) use passthrough::{
    GitIdentityPassthrough, SshPassthroughGuard, apply_git_and_ssh_passthrough,
};
