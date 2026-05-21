// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ssh_signing::known_hosts) enum KnownHostsLookupError {
    Unavailable(String),
    Failed(String),
}

pub(in crate::ssh_signing::known_hosts) type KnownHostsLookup =
    std::result::Result<Vec<String>, KnownHostsLookupError>;

pub(super) fn parse_ssh_keygen_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_keygen_output_lines_without_comments() {
        assert_eq!(
            parse_ssh_keygen_lines(
                "# Host github.com found: line 1\ngithub.com ssh-ed25519 AAAA\n\n# another\n|1|hash ssh-rsa BBBB\n"
            ),
            ["github.com ssh-ed25519 AAAA", "|1|hash ssh-rsa BBBB"]
        );
    }
}
