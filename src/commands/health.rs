use comfy_table::{Cell, Table, presets::NOTHING};
use serde::Serialize;

use crate::Error;
use crate::cli::{HealthArgs, OutputFormat};
use crate::error::Result;
use crate::podman::Podman;
use crate::runtime::{HostRuntimeHealthProbe, RuntimeHealthProbe};
use crate::session::{
    SessionDisplay, SessionRecord, SessionStatus, discover_managed_sessions,
    select_stable_id_prefix, sort_session_refs_by_identity,
};

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

fn print_table(rows: &[HealthRow<'_>]) {
    print!("{}", render_table(rows));
}

fn print_json(rows: &[HealthRow<'_>]) -> Result<()> {
    print!("{}", render_json(rows)?);
    Ok(())
}

fn render_table(rows: &[HealthRow<'_>]) -> String {
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
            Cell::new(row.display.id_or_unknown()),
            Cell::new(row.display.canonical_git_root_or_unknown()),
            Cell::new(row.display.runtime_or_unknown()),
            Cell::new(row.health.as_str()),
            Cell::new(&row.reason),
            Cell::new(row.display.endpoint_or_unknown()),
        ]);
    }

    table::render_table(table)
}

fn render_json(rows: &[HealthRow<'_>]) -> Result<String> {
    let rows = rows.iter().map(HealthJsonRow::from).collect::<Vec<_>>();

    Ok(format!("{}\n", serde_json::to_string(&rows)?))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HealthRow<'a> {
    display: SessionDisplay<'a>,
    health: HealthStatus,
    reason: String,
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
    let selection_id = selection.id().to_string();
    let Some(session) = selection.into_single_session() else {
        return Err(Error::msg(format!(
            "stable id `{selection_id}` matches multiple managed sessions; health requires a single running session",
        )));
    };
    if session.status != SessionStatus::Running {
        return Err(Error::msg(format!(
            "managed session `{}` is `{}`; health only probes running sessions",
            session.stable_id().unwrap_or(&selection_id),
            session.status.as_str()
        )));
    }

    Ok(vec![session])
}

fn health_rows<'a>(
    mut sessions: Vec<&'a SessionRecord>,
    probe: &impl RuntimeHealthProbe,
) -> Vec<HealthRow<'a>> {
    sort_session_refs_by_identity(&mut sessions);

    sessions
        .into_iter()
        .map(|session| health_row(session, probe))
        .collect()
}

fn health_row<'a>(session: &'a SessionRecord, probe: &impl RuntimeHealthProbe) -> HealthRow<'a> {
    let display = session.display();

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
        display,
        health,
        reason,
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

impl<'row, 'session: 'row> From<&'row HealthRow<'session>> for HealthJsonRow<'row> {
    fn from(row: &'row HealthRow<'session>) -> Self {
        Self {
            id: row.display.id(),
            canonical_git_root: row.display.canonical_git_root_str(),
            runtime: row.display.runtime(),
            health: row.health.as_str(),
            reason: &row.reason,
            endpoint: row.display.endpoint(),
            container_name: row.display.container_name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;

    use crate::runtime::{AttachEndpoint, RuntimeHealth, RuntimeKind};
    use crate::session::{SessionMetadata, SessionRecord, SessionStatus};

    use super::*;

    #[test]
    fn health_rendering_uses_shared_session_display_fallbacks() {
        let session = SessionRecord {
            container_id: "container-id".to_string(),
            container_name: "broken-container".to_string(),
            metadata: SessionMetadata::from_labels(&BTreeMap::new()),
            runtime_kind: None,
            attach_endpoint: None,
            container_running: true,
            status: SessionStatus::Running,
        };

        let rows = health_rows(vec![&session], &UnusedProbe);
        let table = render_table(&rows);

        assert!(table.contains("unknown"));
        assert!(table.contains("missing runtime metadata"));
        assert!(!table.contains("broken-container"));

        let json = render_json(&rows).unwrap();
        let rows: Vec<Value> = serde_json::from_str(&json).unwrap();

        assert_eq!(rows.len(), 1);
        assert!(rows[0]["id"].is_null());
        assert!(rows[0]["canonical_git_root"].is_null());
        assert!(rows[0]["runtime"].is_null());
        assert!(rows[0]["endpoint"].is_null());
        assert_eq!(rows[0]["health"], "unhealthy");
        assert_eq!(rows[0]["reason"], "missing runtime metadata");
        assert_eq!(rows[0]["container_name"], "broken-container");
    }

    struct UnusedProbe;

    impl RuntimeHealthProbe for UnusedProbe {
        fn check(&self, _runtime: RuntimeKind, _endpoint: &AttachEndpoint) -> RuntimeHealth {
            panic!("probe should not run for missing runtime metadata")
        }
    }
}
