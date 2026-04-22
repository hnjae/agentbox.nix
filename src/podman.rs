// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::collections::BTreeMap;

use camino::Utf8Path;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::process::ProcessRunner;
use crate::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Podman {
    runner: ProcessRunner,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanPsContainer {
    pub id: String,
    pub image: String,
    #[serde(default, deserialize_with = "deserialize_option_vec_or_string")]
    pub command: Option<Vec<String>>,
    // `podman ps --format json` keeps the stable numeric timestamp in `Created`
    // and also returns a derived human-readable `CreatedAt` string.
    pub created: i64,
    pub created_at: String,
    #[serde(default)]
    pub names: Option<Vec<String>>,
    #[serde(default)]
    pub ports: Option<Vec<PodmanPsPort>>,
    pub status: String,
    pub state: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub mounts: Option<Vec<String>>,
    #[serde(default)]
    pub networks: Option<Vec<String>>,
    #[serde(default)]
    pub namespaces: Option<PodmanNamespaces>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PodmanPsPort {
    #[serde(default)]
    pub container_port: u16,
    #[serde(default)]
    pub host_ip: Option<String>,
    #[serde(default)]
    pub host_port: Option<u16>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub range: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanNamespaces {
    #[serde(default)]
    pub cgroup: Option<String>,
    #[serde(default)]
    pub ipc: Option<String>,
    #[serde(default)]
    pub mnt: Option<String>,
    #[serde(default)]
    pub net: Option<String>,
    #[serde(default)]
    pub pidns: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub uts: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanContainerInspect {
    pub id: String,
    pub created: String,
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub state: PodmanContainerState,
    pub image_name: String,
    pub config: PodmanContainerConfig,
    pub host_config: PodmanHostConfig,
    #[serde(default)]
    pub mounts: Vec<PodmanContainerMount>,
    pub network_settings: PodmanNetworkSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanContainerState {
    pub status: String,
    pub running: bool,
    #[serde(default)]
    pub exit_code: i64,
    #[serde(default)]
    pub pid: i64,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
    // Prefer the structured JSON `State.Health` data over legacy template aliases.
    #[serde(default)]
    pub health: Option<PodmanHealth>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanHealth {
    pub status: String,
    #[serde(default)]
    pub failing_streak: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanContainerConfig {
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub cmd: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default, deserialize_with = "deserialize_option_vec_or_string")]
    pub entrypoint: Option<Vec<String>>,
    #[serde(default)]
    pub stop_signal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanHostConfig {
    #[serde(default)]
    pub auto_remove: bool,
    #[serde(default)]
    pub network_mode: Option<String>,
    #[serde(default)]
    pub privileged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanContainerMount {
    #[serde(rename = "Type")]
    pub kind: String,
    pub source: String,
    pub destination: String,
    #[serde(default)]
    pub rw: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanNetworkSettings {
    #[serde(default)]
    pub networks: BTreeMap<String, PodmanNetworkEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanNetworkEndpoint {
    #[serde(default)]
    #[serde(rename = "IPAddress")]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl Podman {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_runner(runner: ProcessRunner) -> Self {
        Self { runner }
    }

    pub fn ps(&self) -> Result<Vec<PodmanPsContainer>> {
        let output = self.runner.capture("podman", |command| {
            command.args(["ps", "--format", "json"]);
        })?;

        parse_json("`podman ps --format json`", &output.stdout)
    }

    pub fn inspect(&self, name: &str) -> Result<Vec<PodmanContainerInspect>> {
        let output = self.runner.capture("podman", |command| {
            command.args(["inspect", name]);
        })?;

        parse_json("`podman inspect`", &output.stdout)
    }

    pub fn inspect_one(&self, name: &str) -> Result<PodmanContainerInspect> {
        let mut containers = self.inspect(name)?;
        if containers.is_empty() {
            return Err(Error::msg(format!(
                "`podman inspect` returned no containers for `{name}`"
            )));
        }

        Ok(containers.remove(0))
    }

    pub fn image_exists(&self, image: &str) -> Result<bool> {
        let status = self.runner.status("podman", |command| {
            command.args(["image", "exists", image]);
        })?;

        match status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            Some(code) => Err(Error::msg(format!(
                "`podman image exists {image}` exited with exit status {code}"
            ))),
            None => Err(Error::msg(format!(
                "`podman image exists {image}` exited with signal"
            ))),
        }
    }

    pub fn build_image(
        &self,
        image: &str,
        containerfile: &Utf8Path,
        context_dir: &Utf8Path,
    ) -> Result<()> {
        self.runner
            .capture("podman", |command| {
                command.args([
                    "build",
                    "-t",
                    image,
                    "-f",
                    containerfile.as_str(),
                    context_dir.as_str(),
                ]);
            })
            .map(|_| ())
    }
}

fn parse_json<T: DeserializeOwned>(context: &str, input: &str) -> Result<T> {
    serde_json::from_str(input)
        .map_err(|error| Error::msg(format!("failed to parse {context}: {error}")))
}

fn deserialize_option_vec_or_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        String(String),
        Vec(Vec<String>),
    }

    Ok(match Option::<StringOrVec>::deserialize(deserializer)? {
        Some(StringOrVec::String(value)) => Some(vec![value]),
        Some(StringOrVec::Vec(values)) => Some(values),
        None => None,
    })
}

#[allow(dead_code)]
fn deserialize_option_vec<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Option<Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<Vec<T>>::deserialize(deserializer)
}
