// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

const BACKUP_TIMESTAMP_FORMAT: &[FormatItem<'_>] =
    format_description!("[year][month][day]T[hour][minute][second]Z");

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentboxConfig {
    pub known_hosts: Vec<String>,
    pub default_resource_limits: ResourceLimits,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceLimits {
    pub cpus: Option<CpuLimit>,
    pub memory: Option<MemoryLimit>,
}

impl ResourceLimits {
    pub fn overlay(self, overrides: ResourceLimitOverrides) -> Self {
        Self {
            cpus: overrides.cpus.or(self.cpus),
            memory: overrides.memory.or(self.memory),
        }
    }

    pub fn stored_or_zero(&self) -> StoredResourceLimitLabels {
        StoredResourceLimitLabels {
            cpus: self
                .cpus
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "0".to_string()),
            memory: self
                .memory
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "0".to_string()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceLimitOverrides {
    pub cpus: Option<CpuLimit>,
    pub memory: Option<MemoryLimit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredResourceLimitLabels {
    pub cpus: String,
    pub memory: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuLimit(String);

impl CpuLimit {
    pub fn is_unlimited(&self) -> bool {
        self.0 == "0"
    }
}

impl std::fmt::Display for CpuLimit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for CpuLimit {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let trimmed = value.trim();
        if trimmed != value || trimmed.is_empty() {
            return Err("CPU limit must be a non-negative decimal number".to_string());
        }
        let parsed: f64 = trimmed
            .parse()
            .map_err(|_| "CPU limit must be a non-negative decimal number".to_string())?;
        if !parsed.is_finite() || parsed < 0.0 {
            return Err("CPU limit must be a non-negative decimal number".to_string());
        }
        Ok(Self(trimmed.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryLimit(String);

impl MemoryLimit {
    pub fn is_unlimited(&self) -> bool {
        self.0 == "0"
    }
}

impl std::fmt::Display for MemoryLimit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for MemoryLimit {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let trimmed = value.trim();
        if trimmed != value || trimmed.is_empty() {
            return Err("memory limit must use Podman format <number>[b|k|m|g]".to_string());
        }
        let number = trimmed
            .strip_suffix(['b', 'k', 'm', 'g'])
            .unwrap_or(trimmed);
        if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
            return Err("memory limit must use Podman format <number>[b|k|m|g]".to_string());
        }
        Ok(Self(trimmed.to_string()))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgentboxConfig {
    #[serde(default)]
    #[serde(rename = "knownHosts")]
    known_hosts: Vec<String>,
    #[serde(default)]
    #[serde(rename = "defaultResourceLimits")]
    default_resource_limits: RawResourceLimits,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResourceLimits {
    cpus: Option<serde_json::Value>,
    memory: Option<String>,
}

pub fn load_config<W>(warning: &mut W) -> AgentboxConfig
where
    W: FnMut(String) + ?Sized,
{
    load_config_with(
        &mut |name| std::env::var_os(name),
        OffsetDateTime::now_utc(),
        warning,
    )
}

pub fn default_config_contents() -> &'static str {
    "{\n  \"knownHosts\": [],\n  \"defaultResourceLimits\": {}\n}\n"
}

pub fn config_file_path() -> Option<PathBuf> {
    config_path(&mut |name| std::env::var_os(name))
}

pub fn write_default_config(force: bool) -> std::io::Result<PathBuf> {
    let path = config_file_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine agentbox config path because neither XDG_CONFIG_HOME nor HOME is set",
        )
    })?;
    write_default_config_to_path(&path, force)?;
    Ok(path)
}

fn write_default_config_to_path(path: &Path, force: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut options = fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }

    let mut file = options.open(path)?;
    file.write_all(default_config_contents().as_bytes())
}

pub fn load_config_with<E, W>(
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
        default_resource_limits: parse_resource_limits(raw.default_resource_limits)?,
    })
}

fn parse_resource_limits(raw: RawResourceLimits) -> std::result::Result<ResourceLimits, String> {
    Ok(ResourceLimits {
        cpus: raw.cpus.map(parse_cpu_value).transpose()?,
        memory: raw
            .memory
            .map(|value| value.parse::<MemoryLimit>())
            .transpose()?,
    })
}

fn parse_cpu_value(value: serde_json::Value) -> std::result::Result<CpuLimit, String> {
    match value {
        serde_json::Value::Number(number) => number.to_string().parse(),
        serde_json::Value::String(value) => value.parse(),
        _ => Err("CPU limit must be a non-negative decimal number".to_string()),
    }
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
    fn valid_resource_limit_only_config_loads() {
        let config = parse_config(r#"{"defaultResourceLimits":{"cpus":2,"memory":"8g"}}"#).unwrap();

        assert!(config.known_hosts.is_empty());
        assert_eq!(
            config.default_resource_limits.cpus.unwrap().to_string(),
            "2"
        );
        assert_eq!(
            config.default_resource_limits.memory.unwrap().to_string(),
            "8g"
        );
    }

    #[test]
    fn valid_combined_config_loads() {
        let config = parse_config(
            r#"{"knownHosts":["github.com ssh-ed25519 AAAA"],"defaultResourceLimits":{"cpus":1.5,"memory":"512m"}}"#,
        )
        .unwrap();

        assert_eq!(config.known_hosts, ["github.com ssh-ed25519 AAAA"]);
        assert_eq!(
            config.default_resource_limits.cpus.unwrap().to_string(),
            "1.5"
        );
        assert_eq!(
            config.default_resource_limits.memory.unwrap().to_string(),
            "512m"
        );
    }

    #[test]
    fn zero_resource_limits_are_valid() {
        let config = parse_config(r#"{"defaultResourceLimits":{"cpus":0,"memory":"0"}}"#).unwrap();

        assert!(config.default_resource_limits.cpus.unwrap().is_unlimited());
        assert!(
            config
                .default_resource_limits
                .memory
                .unwrap()
                .is_unlimited()
        );
    }

    #[test]
    fn invalid_resource_limits_are_rejected() {
        assert!(parse_config(r#"{"defaultResourceLimits":{"cpus":-1}}"#).is_err());
        assert!(parse_config(r#"{"defaultResourceLimits":{"cpus":"nan"}}"#).is_err());
        assert!(parse_config(r#"{"defaultResourceLimits":{"memory":"1t"}}"#).is_err());
        assert!(parse_config(r#"{"defaultResourceLimits":{"memory":"1.5g"}}"#).is_err());
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
