use comfy_table::{Cell, Table, presets::NOTHING};
use serde::Serialize;

use crate::cli::{HealthArgs, OutputFormat};
use crate::error::Result;
use crate::podman::Podman;
use crate::session::{SessionRecord, SessionStatus, discover_managed_sessions};

use super::runtime_health::{HostRuntimeHealthProbe, RuntimeHealthProbe};

pub fn run(args: HealthArgs) -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_managed_sessions(&podman)?;
    let rows = health_rows(&sessions, &HostRuntimeHealthProbe);
    match args.output {
        OutputFormat::Table => print_table(&rows),
        OutputFormat::Json => print_json(&rows)?,
    }
    Ok(())
}

pub fn print_table(rows: &[HealthRow]) {
    print!("{}", render_table(rows));
}

pub fn print_json(rows: &[HealthRow]) -> Result<()> {
    print!("{}", render_json(rows)?);
    Ok(())
}

pub fn render_table(rows: &[HealthRow]) -> String {
    let mut table = Table::new();
    table.load_preset(NOTHING);
    table.set_header([
        "canonical git root",
        "runtime",
        "health",
        "reason",
        "endpoint",
        "container name",
    ]);

    for row in rows {
        table.add_row([
            Cell::new(row.canonical_git_root.as_deref().unwrap_or("unknown")),
            Cell::new(row.runtime.as_deref().unwrap_or("unknown")),
            Cell::new(row.health.as_str()),
            Cell::new(&row.reason),
            Cell::new(row.endpoint.as_deref().unwrap_or("unknown")),
            Cell::new(&row.container_name),
        ]);
    }

    table.to_string()
}

pub fn render_json(rows: &[HealthRow]) -> Result<String> {
    let rows = rows.iter().map(HealthJsonRow::from).collect::<Vec<_>>();

    Ok(format!("{}\n", serde_json::to_string(&rows)?))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthRow {
    canonical_git_root: Option<String>,
    runtime: Option<String>,
    health: HealthStatus,
    reason: String,
    endpoint: Option<String>,
    container_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HealthStatus {
    Healthy,
    Unhealthy,
}

impl HealthStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Unhealthy => "unhealthy",
        }
    }
}

fn health_rows(sessions: &[SessionRecord], probe: &impl RuntimeHealthProbe) -> Vec<HealthRow> {
    let mut sessions = sessions
        .iter()
        .filter(|session| session.status == SessionStatus::Running)
        .collect::<Vec<_>>();

    sessions.sort_by(|left, right| {
        left.canonical_git_root()
            .map(|root| root.as_str())
            .cmp(&right.canonical_git_root().map(|root| root.as_str()))
            .then_with(|| left.container_name.cmp(&right.container_name))
    });

    sessions
        .into_iter()
        .map(|session| health_row(session, probe))
        .collect()
}

fn health_row(session: &SessionRecord, probe: &impl RuntimeHealthProbe) -> HealthRow {
    let canonical_git_root = session.canonical_git_root().map(ToString::to_string);
    let runtime = session
        .runtime_kind()
        .map(|runtime| runtime.as_str().to_string())
        .or_else(|| session.runtime().map(ToString::to_string));
    let endpoint = session.attach_endpoint.as_ref().map(ToString::to_string);

    let (health, reason) = match (session.runtime_kind(), session.attach_endpoint.as_ref()) {
        (Some(runtime), Some(endpoint)) => {
            let health = probe.check(runtime, endpoint);
            if health.is_healthy() {
                (HealthStatus::Healthy, "ok".to_string())
            } else {
                (HealthStatus::Unhealthy, health.reason().to_string())
            }
        }
        (None, _) => (
            HealthStatus::Unhealthy,
            "missing runtime metadata".to_string(),
        ),
        (_, None) => (
            HealthStatus::Unhealthy,
            "missing attach endpoint".to_string(),
        ),
    };

    HealthRow {
        canonical_git_root,
        runtime,
        health,
        reason,
        endpoint,
        container_name: session.container_name.clone(),
    }
}

#[derive(Debug, Serialize)]
struct HealthJsonRow<'a> {
    canonical_git_root: Option<&'a str>,
    runtime: Option<&'a str>,
    health: &'static str,
    reason: &'a str,
    endpoint: Option<&'a str>,
    container_name: &'a str,
}

impl<'a> From<&'a HealthRow> for HealthJsonRow<'a> {
    fn from(row: &'a HealthRow) -> Self {
        Self {
            canonical_git_root: row.canonical_git_root.as_deref(),
            runtime: row.runtime.as_deref(),
            health: row.health.as_str(),
            reason: &row.reason,
            endpoint: row.endpoint.as_deref(),
            container_name: &row.container_name,
        }
    }
}
