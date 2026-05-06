use comfy_table::{Cell, Table, presets::NOTHING};
use serde::Serialize;

use crate::Error;
use crate::cli::{HealthArgs, OutputFormat};
use crate::error::Result;
use crate::podman::Podman;
use crate::session::{
    SessionRecord, SessionStatus, discover_managed_sessions, select_stable_id_prefix,
    sort_session_refs_by_identity,
};

use super::runtime_health::{HostRuntimeHealthProbe, RuntimeHealthProbe};
use super::table;

pub fn run(args: HealthArgs) -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_managed_sessions(&podman)?;
    let sessions = selected_health_sessions(&sessions, args.target.as_deref())?;
    let rows = health_rows(sessions, &HostRuntimeHealthProbe);
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
        "ID",
        "CANONICAL GIT ROOT",
        "RUNTIME",
        "HEALTH",
        "REASON",
        "ENDPOINT",
    ]);

    for row in rows {
        table.add_row([
            Cell::new(row.id.as_deref().unwrap_or("unknown")),
            Cell::new(row.canonical_git_root.as_deref().unwrap_or("unknown")),
            Cell::new(row.runtime.as_deref().unwrap_or("unknown")),
            Cell::new(row.health.as_str()),
            Cell::new(&row.reason),
            Cell::new(row.endpoint.as_deref().unwrap_or("unknown")),
        ]);
    }

    table::render_table(table)
}

pub fn render_json(rows: &[HealthRow]) -> Result<String> {
    let rows = rows.iter().map(HealthJsonRow::from).collect::<Vec<_>>();

    Ok(format!("{}\n", serde_json::to_string(&rows)?))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthRow {
    id: Option<String>,
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

fn selected_health_sessions<'a>(
    sessions: &'a [SessionRecord],
    target: Option<&str>,
) -> Result<Vec<&'a SessionRecord>> {
    let Some(target) = target else {
        return Ok(sessions
            .iter()
            .filter(|session| session.status == SessionStatus::Running)
            .collect());
    };

    let selection = select_stable_id_prefix(sessions, target)?;
    if selection.sessions().len() != 1 {
        return Err(Error::msg(format!(
            "stable id `{}` matches multiple managed sessions; health requires a single running session",
            selection.id()
        )));
    }

    let session = selection.into_sessions().remove(0);
    if session.status != SessionStatus::Running {
        return Err(Error::msg(format!(
            "managed session `{}` is `{}`; health only probes running sessions",
            session.stable_id().unwrap_or(target),
            session.status.as_str()
        )));
    }

    Ok(vec![session])
}

fn health_rows(
    mut sessions: Vec<&SessionRecord>,
    probe: &impl RuntimeHealthProbe,
) -> Vec<HealthRow> {
    sort_session_refs_by_identity(&mut sessions);

    sessions
        .into_iter()
        .map(|session| health_row(session, probe))
        .collect()
}

fn health_row(session: &SessionRecord, probe: &impl RuntimeHealthProbe) -> HealthRow {
    let display = session.display();
    let id = display.id().map(ToString::to_string);
    let canonical_git_root = display.canonical_git_root_str().map(ToString::to_string);
    let runtime = display.runtime().map(ToString::to_string);
    let endpoint = display.endpoint().map(ToString::to_string);

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
        id,
        canonical_git_root,
        runtime,
        health,
        reason,
        endpoint,
        container_name: display.container_name().to_string(),
    }
}

#[derive(Debug, Serialize)]
struct HealthJsonRow<'a> {
    id: Option<&'a str>,
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
            id: row.id.as_deref(),
            canonical_git_root: row.canonical_git_root.as_deref(),
            runtime: row.runtime.as_deref(),
            health: row.health.as_str(),
            reason: &row.reason,
            endpoint: row.endpoint.as_deref(),
            container_name: &row.container_name,
        }
    }
}
