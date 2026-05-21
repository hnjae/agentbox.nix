// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::path::PathBuf;

use crate::ssh_signing::known_hosts::remote::{SshRemoteHost, format_known_host, parse_port};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ssh_signing::known_hosts) struct SshHostConfig {
    pub(in crate::ssh_signing::known_hosts) lookup_hosts: Vec<String>,
    pub(in crate::ssh_signing::known_hosts) known_hosts_files: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ssh_signing::known_hosts) enum SshConfigLookupError {
    Unavailable(String),
    Failed(String),
}

pub(in crate::ssh_signing::known_hosts) type SshConfigLookup =
    std::result::Result<SshHostConfig, SshConfigLookupError>;

pub(super) fn fallback_ssh_host_config(
    host: &SshRemoteHost,
    home: Option<OsString>,
) -> SshHostConfig {
    SshHostConfig {
        lookup_hosts: vec![host.known_hosts_name()],
        known_hosts_files: known_hosts_files(home),
    }
}

fn known_hosts_files(home: Option<OsString>) -> Vec<PathBuf> {
    let Some(home) = home.filter(|home| !home.is_empty()) else {
        return Vec::new();
    };
    let ssh_dir = PathBuf::from(home).join(".ssh");
    ["known_hosts", "known_hosts2"]
        .into_iter()
        .map(|name| ssh_dir.join(name))
        .filter(|path| path.is_file())
        .collect()
}

pub(super) fn parse_ssh_config_output(output: &str, host: &SshRemoteHost) -> SshHostConfig {
    let mut hostname = None;
    let mut port = host.port;
    let mut host_key_alias = None;
    let mut known_hosts_files = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some((key, values)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let values = values.trim();
        match key.to_ascii_lowercase().as_str() {
            "hostname" => hostname = first_ssh_config_value(values).map(ToOwned::to_owned),
            "port" => port = first_ssh_config_value(values).and_then(parse_port),
            "hostkeyalias" => {
                host_key_alias = first_ssh_config_value(values).map(ToOwned::to_owned);
            }
            "userknownhostsfile" | "globalknownhostsfile" => {
                for value in values.split_whitespace() {
                    push_known_hosts_file(&mut known_hosts_files, value);
                }
            }
            _ => {}
        }
    }

    SshHostConfig {
        lookup_hosts: ssh_config_lookup_hosts(
            host,
            hostname.as_deref(),
            port,
            host_key_alias.as_deref(),
        ),
        known_hosts_files,
    }
}

fn first_ssh_config_value(values: &str) -> Option<&str> {
    values
        .split_whitespace()
        .find(|value| !value.is_empty() && *value != "none")
}

fn push_known_hosts_file(files: &mut Vec<PathBuf>, value: &str) {
    if value == "none" {
        return;
    }

    let path = PathBuf::from(value);
    if path.is_file() && !files.contains(&path) {
        files.push(path);
    }
}

fn ssh_config_lookup_hosts(
    host: &SshRemoteHost,
    hostname: Option<&str>,
    port: Option<u16>,
    host_key_alias: Option<&str>,
) -> Vec<String> {
    let mut hosts = Vec::new();
    push_unique_host(&mut hosts, host_key_alias.map(ToOwned::to_owned));
    let resolved_host = hostname.unwrap_or(&host.config_host);
    push_unique_host(&mut hosts, Some(format_known_host(resolved_host, port)));
    push_unique_host(&mut hosts, Some(host.known_hosts_name()));
    hosts
}

fn push_unique_host(hosts: &mut Vec<String>, host: Option<String>) {
    let Some(host) = host.filter(|host| !host.is_empty()) else {
        return;
    };
    if !hosts.contains(&host) {
        hosts.push(host);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn parses_ssh_config_lookup_hosts_and_files() {
        let sandbox = tempfile::tempdir().unwrap();
        let user_file = sandbox.path().join("known_hosts.custom");
        let global_file = sandbox.path().join("ssh_known_hosts");
        fs::write(&user_file, "").unwrap();
        fs::write(&global_file, "").unwrap();
        let remote_host = SshRemoteHost {
            config_host: "github-work".to_string(),
            port: None,
        };
        let output = format!(
            "host github-work\nhostname ssh.github.com\nport 443\nhostkeyalias github.com\nuserknownhostsfile {} missing\nuserknownhostsfile none\nglobalknownhostsfile {}\n",
            user_file.display(),
            global_file.display()
        );

        let config = parse_ssh_config_output(&output, &remote_host);

        assert_eq!(
            config.lookup_hosts,
            ["github.com", "[ssh.github.com]:443", "github-work"]
        );
        assert_eq!(config.known_hosts_files, [user_file, global_file]);
    }
}
