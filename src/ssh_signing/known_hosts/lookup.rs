// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::path::Path;

mod commands;
mod config;
mod keygen;

pub(super) use commands::{ssh_config_lookup, ssh_keygen_lookup};
#[cfg(test)]
pub(super) use config::SshHostConfig;
pub(super) use config::{SshConfigLookup, SshConfigLookupError};
pub(super) use keygen::{KnownHostsLookup, KnownHostsLookupError};

use super::remote::SshRemoteHost;

pub(super) fn host_known_hosts_lines<W>(
    remote_hosts: &[SshRemoteHost],
    home: Option<OsString>,
    ssh_config: &mut impl FnMut(&SshRemoteHost) -> SshConfigLookup,
    lookup: &mut impl FnMut(&str, &Path) -> KnownHostsLookup,
    warning: &mut W,
) -> Vec<String>
where
    W: FnMut(String) + ?Sized,
{
    if remote_hosts.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for host in remote_hosts {
        let config = match ssh_config(host) {
            Ok(config) => config,
            Err(SshConfigLookupError::Unavailable(reason)) => {
                warning(format!(
                    "ssh is unavailable for SSH config lookup for remote host `{}` ({reason}); falling back to $HOME/.ssh/known_hosts and $HOME/.ssh/known_hosts2",
                    host.known_hosts_name()
                ));
                config::fallback_ssh_host_config(host, home.clone())
            }
            Err(SshConfigLookupError::Failed(reason)) => {
                warning(format!(
                    "ssh -G lookup failed for SSH remote host `{}` ({reason}); falling back to $HOME/.ssh/known_hosts and $HOME/.ssh/known_hosts2",
                    host.known_hosts_name()
                ));
                config::fallback_ssh_host_config(host, home.clone())
            }
        };
        let mut host_lines = Vec::new();
        for file in &config.known_hosts_files {
            for lookup_host in &config.lookup_hosts {
                match lookup(lookup_host, file) {
                    Ok(matches) => host_lines.extend(matches),
                    Err(KnownHostsLookupError::Unavailable(reason)) => {
                        warning(format!(
                            "ssh-keygen is unavailable for SSH known_hosts lookup ({reason}); continuing with config-provided knownHosts only"
                        ));
                        return Vec::new();
                    }
                    Err(KnownHostsLookupError::Failed(reason)) => {
                        warning(format!(
                            "ssh-keygen lookup failed for SSH remote host `{}` ({reason}); continuing with config-provided knownHosts only",
                            host.known_hosts_name()
                        ));
                        return Vec::new();
                    }
                }
            }
        }

        if host_lines.is_empty() {
            warning(format!(
                "no known_hosts entry found for SSH remote host `{}`; Git SSH host verification may fail",
                host.known_hosts_name()
            ));
        }
        lines.extend(host_lines);
    }

    lines
}
