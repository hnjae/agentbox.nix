// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

const BACKUP_TIMESTAMP_FORMAT: &[FormatItem<'_>] =
    format_description!("[year][month][day]T[hour][minute][second]Z");

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AgentboxConfig {
    pub(super) known_hosts: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgentboxConfig {
    #[serde(rename = "knownHosts")]
    known_hosts: Vec<String>,
}

pub(super) fn load_config_with<E, W>(
    environment: &mut E,
    now: OffsetDateTime,
    warning: &mut W,
) -> AgentboxConfig
where
    E: FnMut(&str) -> Option<OsString> + ?Sized,
    W: FnMut(String) + ?Sized,
{
    let Some(path) = config_path(environment) else {
        warning(
            "cannot determine agentbox config path because neither XDG_CONFIG_HOME nor HOME is set; continuing with empty config".to_string(),
        );
        return AgentboxConfig::default();
    };

    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return AgentboxConfig::default();
        }
        Err(error) => {
            warning(format!(
                "failed to read agentbox config `{}`: {error}; continuing with empty config",
                path.display()
            ));
            return AgentboxConfig::default();
        }
    };

    match parse_config(&contents) {
        Ok(config) => config,
        Err(reason) => {
            let backup = backup_incompatible_config(&path, now);
            match backup {
                Ok(backup) => warning(format!(
                    "agentbox config `{}` is incompatible ({reason}); backed it up to `{}` and continuing with empty config",
                    path.display(),
                    backup.display()
                )),
                Err(error) => warning(format!(
                    "agentbox config `{}` is incompatible ({reason}); failed to back it up ({error}) and continuing with empty config",
                    path.display()
                )),
            }
            AgentboxConfig::default()
        }
    }
}

fn config_path<E>(environment: &mut E) -> Option<PathBuf>
where
    E: FnMut(&str) -> Option<OsString> + ?Sized,
{
    environment("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            environment("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .map(|config_home| config_home.join("agentbox/config.json"))
}

fn parse_config(contents: &str) -> std::result::Result<AgentboxConfig, String> {
    let raw: RawAgentboxConfig =
        serde_json::from_str(contents).map_err(|error| error.to_string())?;
    for entry in &raw.known_hosts {
        if entry.trim().is_empty() {
            return Err("knownHosts entries must not be blank".to_string());
        }
        if entry.contains('\n') || entry.contains('\r') {
            return Err("knownHosts entries must be single-line strings".to_string());
        }
    }
    Ok(AgentboxConfig {
        known_hosts: raw.known_hosts,
    })
}

fn backup_incompatible_config(path: &Path, now: OffsetDateTime) -> std::io::Result<PathBuf> {
    let timestamp = backup_timestamp(now);
    for suffix in std::iter::once(None).chain((1..).map(Some)) {
        let backup = backup_path(path, &timestamp, suffix);
        if backup.exists() {
            continue;
        }

        fs::rename(path, &backup)?;
        return Ok(backup);
    }

    unreachable!("unbounded suffix iterator must return a backup path")
}

fn backup_path(path: &Path, timestamp: &str, suffix: Option<usize>) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "config.json".into());
    let backup_name = match suffix {
        Some(suffix) => format!("{name}.bak.{timestamp}.{suffix}"),
        None => format!("{name}.bak.{timestamp}"),
    };
    path.with_file_name(backup_name)
}

fn backup_timestamp(now: OffsetDateTime) -> String {
    now.to_offset(UtcOffset::UTC)
        .format(BACKUP_TIMESTAMP_FORMAT)
        .unwrap_or_else(|_| "00000000T000000Z".to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::*;

    #[test]
    fn missing_config_returns_empty_config() {
        let sandbox = tempfile::tempdir().unwrap();
        let mut warnings = Vec::new();

        let config = load_config_with(
            &mut |name| test_config_env(name, sandbox.path()),
            sample_time(),
            &mut |warning| warnings.push(warning),
        );

        assert_eq!(config, AgentboxConfig::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn valid_known_hosts_config_loads() {
        let sandbox = tempfile::tempdir().unwrap();
        let config_path = sandbox.path().join("agentbox/config.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            r#"{"knownHosts":["github.com ssh-ed25519 AAAA","[git.example.com]:2222 ssh-ed25519 BBBB"]}"#,
        )
        .unwrap();

        let config = load_config_with(
            &mut |name| test_config_env(name, sandbox.path()),
            sample_time(),
            &mut panic_warning,
        );

        assert_eq!(
            config.known_hosts,
            [
                "github.com ssh-ed25519 AAAA",
                "[git.example.com]:2222 ssh-ed25519 BBBB"
            ]
        );
    }

    #[test]
    fn invalid_config_is_backed_up_and_ignored() {
        let sandbox = tempfile::tempdir().unwrap();
        let config_path = sandbox.path().join("agentbox/config.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, "{not json").unwrap();
        let mut warnings = Vec::new();

        let config = load_config_with(
            &mut |name| test_config_env(name, sandbox.path()),
            sample_time(),
            &mut |warning| warnings.push(warning),
        );

        let backup = sandbox
            .path()
            .join("agentbox/config.json.bak.20260517T112345Z");
        assert_eq!(config, AgentboxConfig::default());
        assert!(!config_path.exists());
        assert_eq!(fs::read_to_string(backup).unwrap(), "{not json");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("incompatible"));
    }

    #[test]
    fn invalid_config_backup_collision_appends_numeric_suffix() {
        let sandbox = tempfile::tempdir().unwrap();
        let config_path = sandbox.path().join("agentbox/config.json");
        let first_backup = sandbox
            .path()
            .join("agentbox/config.json.bak.20260517T112345Z");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, r#"{"knownHosts":[""]}"#).unwrap();
        fs::write(&first_backup, "previous").unwrap();

        let config = load_config_with(
            &mut |name| test_config_env(name, sandbox.path()),
            sample_time(),
            &mut |_| {},
        );

        let second_backup = sandbox
            .path()
            .join("agentbox/config.json.bak.20260517T112345Z.1");
        assert_eq!(config, AgentboxConfig::default());
        assert_eq!(fs::read_to_string(first_backup).unwrap(), "previous");
        assert_eq!(
            fs::read_to_string(second_backup).unwrap(),
            r#"{"knownHosts":[""]}"#
        );
    }

    #[test]
    fn config_rejects_unknown_fields_and_multiline_entries() {
        assert!(parse_config(r#"{"knownHosts":[],"extra":true}"#).is_err());
        assert!(parse_config("{\"knownHosts\":[\"github.com ssh-ed25519 AAAA\\nnext\"]}").is_err());
    }

    fn test_config_env(name: &str, config_home: &Path) -> Option<OsString> {
        match name {
            "XDG_CONFIG_HOME" => Some(config_home.as_os_str().to_os_string()),
            _ => None,
        }
    }

    fn sample_time() -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::May, 17).unwrap(),
            Time::from_hms(11, 23, 45).unwrap(),
        )
        .assume_utc()
    }

    fn panic_warning(warning: String) {
        panic!("unexpected warning: {warning}");
    }
}
