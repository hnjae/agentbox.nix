use comfy_table::{Cell, Table, presets::UTF8_FULL};

use crate::error::Result;
use crate::podman::Podman;
use crate::session::{SessionRecord, SessionStatus, discover_managed_sessions};

use super::runtime_health::{HostRuntimeHealthProbe, RuntimeHealthProbe};

pub fn run() -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_managed_sessions(&podman)?;
    let rows = health_rows(&sessions, &HostRuntimeHealthProbe);
    print_table(&rows);
    Ok(())
}

pub fn print_table(rows: &[HealthRow]) {
    print!("{}", render_table(rows));
}

pub fn render_table(rows: &[HealthRow]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
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
            Cell::new(&row.canonical_git_root),
            Cell::new(&row.runtime),
            Cell::new(row.health.as_str()),
            Cell::new(&row.reason),
            Cell::new(&row.endpoint),
            Cell::new(&row.container_name),
        ]);
    }

    table.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthRow {
    canonical_git_root: String,
    runtime: String,
    health: HealthStatus,
    reason: String,
    endpoint: String,
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
    let canonical_git_root = session
        .canonical_git_root()
        .map_or_else(|| "unknown".to_string(), ToString::to_string);
    let runtime = session.runtime_kind().map_or_else(
        || session.runtime().unwrap_or("unknown").to_string(),
        |runtime| runtime.as_str().to_string(),
    );
    let endpoint = session
        .attach_endpoint
        .as_ref()
        .map_or_else(|| "unknown".to_string(), ToString::to_string);

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
