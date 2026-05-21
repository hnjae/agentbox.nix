// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::runtime::{AttachEndpoint, RuntimeAttachSpec};
use crate::{Error, Result};

use super::de::{
    deserialize_map_or_null_default, deserialize_option_string_or_number,
    deserialize_vec_or_null_default,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanNetworkSettings {
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub networks: BTreeMap<String, PodmanNetworkEndpoint>,
    #[serde(default, deserialize_with = "deserialize_map_or_null_default")]
    pub ports: BTreeMap<String, Option<Vec<PodmanPortBinding>>>,
}

impl PodmanNetworkSettings {
    pub fn published_attach_endpoint(
        &self,
        attach: RuntimeAttachSpec,
    ) -> Result<Option<AttachEndpoint>> {
        Ok(self
            .published_tcp_host_port(attach.container_port)?
            .map(|published_port| AttachEndpoint {
                scheme: attach.scheme.to_string(),
                host_ip: published_port.host_ip,
                host_port: published_port.host_port,
            }))
    }

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn published_attach_endpoint_uses_runtime_attach_metadata() {
        let network = network_with_binding(
            "4096/tcp",
            PodmanPortBinding {
                host_ip: Some("127.0.0.2".to_string()),
                host_port: Some("49152".to_string()),
            },
        );

        assert_eq!(
            network
                .published_attach_endpoint(crate::runtime::RuntimeAttachSpec {
                    scheme: "https",
                    container_listen_ip: "127.0.0.1",
                    container_port: 4096,
                })
                .unwrap(),
            Some(crate::runtime::AttachEndpoint {
                scheme: "https".to_string(),
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
