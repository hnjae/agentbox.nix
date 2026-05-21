// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Error, Result};

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanVolume {
    pub name: String,
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub mountpoint: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PodmanImage {
    #[serde(default, rename = "Repository", alias = "repository")]
    pub repository: String,
    #[serde(default, rename = "Tag", alias = "tag")]
    pub tag: String,
    #[serde(
        default,
        rename = "Names",
        alias = "names",
        deserialize_with = "deserialize_option_vec_or_string"
    )]
    pub names: Option<Vec<String>>,
    #[serde(
        default,
        rename = "Labels",
        alias = "labels",
        deserialize_with = "deserialize_map_or_null_default"
    )]
    pub labels: BTreeMap<String, String>,
}

impl PodmanImage {
    pub fn references(&self) -> Vec<String> {
        let mut references = self.names.clone().unwrap_or_default();

        if !self.repository.is_empty()
            && !self.tag.is_empty()
            && self.repository != "<none>"
            && self.tag != "<none>"
        {
            references.push(format!("{}:{}", self.repository, self.tag));
        }

        references.retain(|reference| !reference.is_empty() && reference != "<none>");
        references.sort();
        references.dedup();
        references
    }
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanNetworkSettings {
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub networks: BTreeMap<String, PodmanNetworkEndpoint>,
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub ports: BTreeMap<String, Option<Vec<PodmanPortBinding>>>,
}

impl PodmanNetworkSettings {
    pub fn published_tcp_host_port(
        &self,
        container_port: u16,
    ) -> Result<Option<PodmanPublishedPort>> {
        self.published_host_port(&format!("{container_port}/tcp"))
    }

    fn published_host_port(&self, port_key: &str) -> Result<Option<PodmanPublishedPort>> {
        let Some(binding) = self
            .ports
            .get(port_key)
            .and_then(|bindings| bindings.as_ref())
            .and_then(|bindings| bindings.iter().find(|binding| binding.host_port.is_some()))
        else {
            return Ok(None);
        };

        let host_port = binding
            .host_port
            .as_deref()
            .ok_or_else(|| Error::msg(format!("missing host port for `{port_key}`")))?
            .parse::<u16>()
            .map_err(|error| Error::msg(format!("malformed published host port: {error}")))?;
        let host_ip = binding
            .host_ip
            .as_deref()
            .filter(|host_ip| !host_ip.trim().is_empty())
            .unwrap_or(crate::runtime::DEFAULT_HOST_ATTACH_IP)
            .to_string();

        Ok(Some(PodmanPublishedPort { host_ip, host_port }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodmanPublishedPort {
    pub host_ip: String,
    pub host_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanNetworkEndpoint {
    #[serde(default)]
    #[serde(rename = "IPAddress")]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default, deserialize_with = "deserialize_vec_or_null_default")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanPortBinding {
    #[serde(default)]
    pub host_ip: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_string_or_number")]
    pub host_port: Option<String>,
}

pub(super) fn parse_json<T: DeserializeOwned>(context: &str, input: &str) -> Result<T> {
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

fn deserialize_map_or_null_default<'de, D, K, V>(
    deserializer: D,
) -> std::result::Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Ord + Deserialize<'de>,
    V: Deserialize<'de>,
{
    Ok(Option::<BTreeMap<K, V>>::deserialize(deserializer)?.unwrap_or_default())
}

fn deserialize_vec_or_null_default<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

fn deserialize_option_string_or_number<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Number(u16),
    }

    Ok(match Option::<StringOrNumber>::deserialize(deserializer)? {
        Some(StringOrNumber::String(value)) => Some(value),
        Some(StringOrNumber::Number(value)) => Some(value.to_string()),
        None => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn podman_ps_container_treats_null_labels_as_empty() {
        let containers: Vec<PodmanPsContainer> = serde_json::from_str(
            r#"[
  {
    "Id": "0123456789abcdef",
    "Image": "registry.example/image:latest",
    "Command": null,
    "Created": 1713681300,
    "CreatedAt": "2026-04-21 10:15:00 +0000 UTC",
    "Names": ["ambient-container"],
    "Ports": null,
    "Status": "Exited (0) 1 minute ago",
    "State": "exited",
    "Labels": null,
    "Mounts": null,
    "Networks": null,
    "Namespaces": null
  }
]"#,
        )
        .unwrap();

        assert_eq!(containers.len(), 1);
        assert!(containers[0].labels.is_empty());
    }

    #[test]
    fn published_tcp_host_port_returns_published_binding() {
        let network = network_with_binding(
            "4096/tcp",
            PodmanPortBinding {
                host_ip: Some("127.0.0.2".to_string()),
                host_port: Some("49152".to_string()),
            },
        );

        assert_eq!(
            network.published_tcp_host_port(4096).unwrap(),
            Some(PodmanPublishedPort {
                host_ip: "127.0.0.2".to_string(),
                host_port: 49152,
            })
        );
    }

    #[test]
    fn published_tcp_host_port_defaults_blank_host_ip() {
        let network = network_with_binding(
            "4096/tcp",
            PodmanPortBinding {
                host_ip: Some(" ".to_string()),
                host_port: Some("49152".to_string()),
            },
        );

        assert_eq!(
            network.published_tcp_host_port(4096).unwrap(),
            Some(PodmanPublishedPort {
                host_ip: crate::runtime::DEFAULT_HOST_ATTACH_IP.to_string(),
                host_port: 49152,
            })
        );
    }

    #[test]
    fn published_tcp_host_port_returns_none_for_missing_port() {
        let network = PodmanNetworkSettings {
            networks: BTreeMap::new(),
            ports: BTreeMap::new(),
        };

        assert_eq!(network.published_tcp_host_port(4096).unwrap(), None);
    }

    #[test]
    fn published_tcp_host_port_reports_malformed_host_port() {
        let network = network_with_binding(
            "4096/tcp",
            PodmanPortBinding {
                host_ip: Some("127.0.0.1".to_string()),
                host_port: Some("not-a-port".to_string()),
            },
        );

        let error = network.published_tcp_host_port(4096).unwrap_err();

        assert!(
            error.to_string().contains("malformed published host port"),
            "{error}"
        );
    }

    fn network_with_binding(port_key: &str, binding: PodmanPortBinding) -> PodmanNetworkSettings {
        PodmanNetworkSettings {
            networks: BTreeMap::new(),
            ports: BTreeMap::from([(port_key.to_string(), Some(vec![binding]))]),
        }
    }
}
