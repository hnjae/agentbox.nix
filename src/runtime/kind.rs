// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::str::FromStr;

use clap::ValueEnum;

use crate::{Error, Result};

use super::default_image::{self, DefaultImageBuildContext};
use super::profile::{self, RuntimeHostStateMount, RuntimePackageSpec};
use super::spec::{AttachEndpoint, RuntimeAttachSpec, RuntimeCommand, RuntimeHealthCheck};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum RuntimeKind {
    Opencode,
    Codex,
}

impl RuntimeKind {
    pub fn variants() -> &'static [Self] {
        <Self as ValueEnum>::value_variants()
    }

    pub fn as_str(self) -> &'static str {
        profile::runtime_profile(self).name
    }

    pub fn supported_values_placeholder() -> String {
        profile::supported_runtime_placeholder()
    }

    pub fn supported_values() -> Vec<&'static str> {
        profile::supported_runtime_values()
    }

    pub fn default_image(self) -> String {
        default_image::default_image(self)
    }

    pub fn materialize_default_image_context(self) -> Result<DefaultImageBuildContext> {
        let profile = self.profile();
        (profile.materialize_default_image_context)()
    }

    pub(crate) fn package_spec(self) -> RuntimePackageSpec {
        self.profile().package
    }

    pub(crate) fn host_state_mounts(self) -> &'static [RuntimeHostStateMount] {
        self.profile().host_state_mounts
    }

    pub fn attach_spec(self) -> RuntimeAttachSpec {
        self.profile().attach
    }

    pub(crate) fn health_check(self) -> RuntimeHealthCheck {
        self.profile().health_check
    }

    pub fn server_command(self) -> RuntimeCommand {
        let profile = self.profile();
        profile.server_command.render(profile.attach)
    }

    pub fn host_client_command(self, endpoint: &AttachEndpoint) -> RuntimeCommand {
        let profile = self.profile();
        profile.host_client_command.render(endpoint)
    }

    pub fn foreground_command(self) -> RuntimeCommand {
        self.profile().foreground_command.render()
    }

    pub(super) fn profile(self) -> &'static profile::RuntimeProfile {
        profile::runtime_profile(self)
    }
}

impl fmt::Display for RuntimeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RuntimeKind {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        if value.trim().is_empty() {
            return Err(Error::msg(
                "malformed runtime label: `io.agentbox.runtime` is empty",
            ));
        }

        profile::runtime_kind_from_name(value).ok_or_else(|| {
            Error::msg(format!(
                "unsupported runtime `{value}`; supported runtimes are {}",
                profile::supported_runtime_names()
            ))
        })
    }
}
