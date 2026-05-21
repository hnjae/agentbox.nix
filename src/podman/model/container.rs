// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::de::{
    deserialize_map_or_null_default, deserialize_option_vec_or_string,
    deserialize_vec_or_null_default,
};
use super::network::PodmanNetworkSettings;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanPsContainer {
    pub id: String,
    pub image: String,
    #[serde(default, deserialize_with = "deserialize_option_vec_or_string")]
    pub command: Option<Vec<String>>,
    // `podman ps --all --format json` keeps the stable numeric timestamp in `Created`
    // and also returns a derived human-readable `CreatedAt` string.
    pub created: i64,
    pub created_at: String,
    #[serde(default)]
    pub names: Option<Vec<String>>,
    #[serde(default)]
    pub ports: Option<Vec<PodmanPsPort>>,
    pub status: String,
    pub state: String,
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub mounts: Option<Vec<String>>,
    #[serde(default)]
    pub networks: Option<Vec<String>>,
    #[serde(default)]
    pub namespaces: Option<PodmanNamespaces>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanContainerInspect {
    pub id: String,
    pub created: String,
    pub path: String,
    #[serde(default, deserialize_with = "deserialize_vec_or_null_default")]
    pub args: Vec<String>,
    pub state: PodmanContainerState,
    pub image_name: String,
    pub config: PodmanContainerConfig,
    pub host_config: PodmanHostConfig,
    #[serde(default, deserialize_with = "deserialize_vec_or_null_default")]
    pub mounts: Vec<PodmanContainerMount>,
    pub network_settings: PodmanNetworkSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanHealth {
    pub status: String,
    #[serde(default)]
    pub failing_streak: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanContainerConfig {
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_null_default")]
    pub env: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_null_default")]
    pub cmd: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub labels: BTreeMap<String, String>,
    #[serde(default, deserialize_with = "deserialize_option_vec_or_string")]
    pub entrypoint: Option<Vec<String>>,
    #[serde(default)]
    pub stop_signal: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanHostConfig {
    #[serde(default)]
    pub auto_remove: bool,
    #[serde(default)]
    pub network_mode: Option<String>,
    #[serde(default)]
    pub privileged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanContainerMount {
    #[serde(rename = "Type")]
    pub kind: PodmanContainerMountKind,
    pub source: String,
    pub destination: String,
    #[serde(default)]
    #[serde(rename = "RW")]
    pub rw: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodmanContainerMountKind {
    Bind,
    Volume,
    Other(String),
}

impl PodmanContainerMountKind {
    pub fn is_volume(&self) -> bool {
        matches!(self, Self::Volume)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Bind => "bind",
            Self::Volume => "volume",
            Self::Other(value) => value,
        }
    }
}

impl<'de> Deserialize<'de> for PodmanContainerMountKind {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let kind = String::deserialize(deserializer)?;

        Ok(match kind.as_str() {
            "bind" => Self::Bind,
            "volume" => Self::Volume,
            other => Self::Other(other.to_string()),
        })
    }
}

impl Serialize for PodmanContainerMountKind {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
