// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

pub mod opencode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeMountKind {
    Bind,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMount {
    pub kind: RuntimeMountKind,
    pub source: String,
    pub destination: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecSpec {
    pub argv: Vec<String>,
    pub detached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCreateSpec {
    pub image: String,
    pub labels: BTreeMap<String, String>,
    pub mounts: Vec<RuntimeMount>,
    pub command: Vec<String>,
    pub default_env: BTreeMap<String, String>,
    pub network_enabled: bool,
    pub published_ports: Vec<String>,
}
