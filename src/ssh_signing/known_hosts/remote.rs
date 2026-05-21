// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct SshRemoteHost {
    pub(super) config_host: String,
    pub(super) port: Option<u16>,
}

impl SshRemoteHost {
    pub(super) fn known_hosts_name(&self) -> String {
        format_known_host(&self.config_host, self.port)
    }
}

pub(super) fn ssh_remote_hosts(urls: &[String]) -> Vec<SshRemoteHost> {
    let mut hosts = Vec::new();
    let mut seen = HashSet::new();
    for url in urls {
        if let Some(host) = ssh_remote_host(url)
            && seen.insert(host.clone())
        {
            hosts.push(host);
        }
    }
    hosts
}

fn ssh_remote_host(url: &str) -> Option<SshRemoteHost> {
    if let Some(rest) = url.strip_prefix("ssh://") {
        return ssh_url_host(rest);
    }
    if let Some(rest) = url.strip_prefix("git+ssh://") {
        return ssh_url_host(rest);
    }

    scp_like_host(url)
}

fn ssh_url_host(rest: &str) -> Option<SshRemoteHost> {
    let authority = rest.split('/').next()?;
    if authority.is_empty() {
        return None;
    }

    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.is_empty() {
        return None;
    }

    if let Some(after_bracket) = authority.strip_prefix('[') {
        let (host, remainder) = after_bracket.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        let port = remainder.strip_prefix(':').and_then(parse_port);
        return Some(SshRemoteHost {
            config_host: host.to_string(),
            port,
        });
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, parse_port(port)),
        _ => (authority, None),
    };
    if host.is_empty() {
        None
    } else {
        Some(SshRemoteHost {
            config_host: host.to_string(),
            port,
        })
    }
}

fn scp_like_host(url: &str) -> Option<SshRemoteHost> {
    if url.contains("://") {
        return None;
    }
    let (host, _path) = url.split_once(':')?;
    if host.is_empty() || host.contains('/') {
        return None;
    }

    let host = host.rsplit('@').next().unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(SshRemoteHost {
            config_host: host.to_string(),
            port: None,
        })
    }
}

pub(super) fn parse_port(port: &str) -> Option<u16> {
    port.parse::<u16>().ok()
}

pub(super) fn format_known_host(host: &str, port: Option<u16>) -> String {
    match port {
        Some(22) | None => host.to_string(),
        Some(port) => format!("[{host}]:{port}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_remote_hosts() {
        let urls = vec![
            "git@github.com:owner/repo.git".to_string(),
            "ssh://git@gitlab.com/group/repo.git".to_string(),
            "ssh://git@example.com:2222/repo.git".to_string(),
            "git+ssh://git@example.net/repo.git".to_string(),
            "https://github.com/owner/repo.git".to_string(),
            "/home/alice/repo".to_string(),
            "../repo".to_string(),
        ];

        assert_eq!(
            ssh_remote_hosts(&urls),
            [
                SshRemoteHost {
                    config_host: "github.com".to_string(),
                    port: None,
                },
                SshRemoteHost {
                    config_host: "gitlab.com".to_string(),
                    port: None,
                },
                SshRemoteHost {
                    config_host: "example.com".to_string(),
                    port: Some(2222),
                },
                SshRemoteHost {
                    config_host: "example.net".to_string(),
                    port: None,
                },
            ]
        );
    }
}
