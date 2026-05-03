// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::Result;

use super::default_image::DefaultImageBuildContext;
use super::kind::RuntimeKind;
use super::profile;
use super::spec::{AttachEndpoint, RuntimeAttachSpec, RuntimeCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeAdapter {
    pub(super) kind: RuntimeKind,
}

impl RuntimeAdapter {
    pub(crate) fn new(kind: RuntimeKind) -> Self {
        Self { kind }
    }

    pub fn name(self) -> &'static str {
        self.kind.as_str()
    }

    pub fn default_image(self) -> &'static str {
        self.profile().default_image
    }

    pub fn materialize_default_image_context(self) -> Result<DefaultImageBuildContext> {
        let profile = self.profile();
        (profile.materialize_default_image_context)()
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
        profile::runtime_profile(self.kind)
    }
}
