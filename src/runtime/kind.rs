// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fmt;
use std::str::FromStr;

use clap::ValueEnum;

use crate::{Error, Result};

use super::RuntimePackageSpec;
use super::default_image::DefaultImageBuildContext;
use super::profile;
use super::spec::{AttachEndpoint, RuntimeAttachSpec, RuntimeCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum RuntimeKind {
    Opencode,
    Codex,
}

impl RuntimeKind {
    pub fn as_str(self) -> &'static str {
        profile::runtime_profile(self).name
    }

    pub fn supported_values_placeholder() -> String {
        profile::supported_runtime_placeholder()
    }

    pub fn supported_values() -> Vec<&'static str> {
        profile::supported_runtime_values()
    }

    pub fn default_image(self) -> &'static str {
        self.profile().default_image
    }

    pub fn materialize_default_image_context(self) -> Result<DefaultImageBuildContext> {
        let profile = self.profile();
        (profile.materialize_default_image_context)()
    }

    pub(crate) fn package_spec(self) -> RuntimePackageSpec {
        self.profile().package
    }

    pub fn attach_spec(self) -> RuntimeAttachSpec {
        self.profile().attach
    }

    pub fn server_command(self) -> RuntimeCommand {
        let profile = self.profile();
        profile.server_command.render(profile.attach)
    }

    pub fn host_client_command(self, endpoint: &AttachEndpoint) -> RuntimeCommand {
        let profile = self.profile();
        profile.host_client_command.render(endpoint)
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
