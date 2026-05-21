// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::Args;
use comfy_table::Cell;
use serde::Serialize;

use crate::error::Result;
use crate::podman::Podman;
use crate::runtime::{HostRuntimeHealthProbe, RuntimeHealth, RuntimeHealthProbe};
use crate::session::{
    HealthSessionTargetPlan, SessionDiscoveryQuery, SessionRecord, sort_session_refs_by_identity,
};

use super::output::{self, OutputFormat};
use super::session_output::{
    SessionDisplay, SessionJsonFields, SessionJsonIdField, SessionJsonMetadataFields,
    SessionJsonTrailingFields, SessionTableFields,
};

#[derive(Debug, Args, PartialEq, Eq)]
pub struct HealthArgs {
    /// Output format.
    #[arg(short = 'o', long = "output", value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,

    /// Stable session id prefix to probe.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,
}

pub fn run(args: HealthArgs) -> Result<()> {
    let podman = Podman::new();
    let sessions = SessionDiscoveryQuery::managed_sessions().discover(&podman)?;
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
    let mut table = output::table([
        "ID",
        "CANONICAL GIT ROOT",
        "RUNTIME",
        "HEALTH",
        "REASON",
        "ENDPOINT",
    ]);

    for row in rows {
        let fields = SessionTableFields::from_display(&row.display);
        table.add_row([
            Cell::new(fields.id),
            Cell::new(fields.canonical_git_root),
            Cell::new(fields.runtime),
            Cell::new(row.health.status_str()),
            Cell::new(row.health.reason()),
            Cell::new(fields.endpoint),
        ]);
    }

    output::render_table(table)
}

fn render_json(rows: &[HealthRow<'_>]) -> Result<String> {
    let rows = rows.iter().map(HealthJsonRow::from).collect::<Vec<_>>();

    output::render_json(&rows)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HealthRow<'a> {
    display: SessionDisplay<'a>,
    health: RuntimeHealth,
}

fn selected_health_sessions<'a>(
    sessions: &'a [SessionRecord],
    target: Option<&str>,
) -> Result<Vec<&'a SessionRecord>> {
    HealthSessionTargetPlan::from_target(target).select_sessions(sessions)
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
    let display = SessionDisplay::from_session(session);

    let health = match (session.runtime_kind(), session.attach_endpoint()) {
        (Some(runtime), Some(endpoint)) => probe.check(runtime, endpoint),
        (None, _) => RuntimeHealth::unhealthy("missing runtime metadata"),
        (_, None) => RuntimeHealth::unhealthy("missing attach endpoint"),
    };

    HealthRow { display, health }
}

#[derive(Debug, Serialize)]
struct HealthJsonRow<'identity, 'row> {
    #[serde(flatten)]
    id: SessionJsonIdField<'identity>,
    #[serde(flatten)]
    metadata: SessionJsonMetadataFields<'identity>,
    health: &'static str,
    reason: &'row str,
    #[serde(flatten)]
    trailing: SessionJsonTrailingFields<'identity>,
}

impl<'row, 'session> From<&'row HealthRow<'session>> for HealthJsonRow<'session, 'row> {
    fn from(row: &'row HealthRow<'session>) -> Self {
        let fields = SessionJsonFields::from_display(&row.display);

        Self {
            id: fields.id,
            metadata: fields.metadata,
            health: row.health.status_str(),
            reason: row.health.reason(),
            trailing: fields.trailing,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;

    use crate::metadata::AgentboxContainerKind;
    use crate::runtime::{AttachEndpoint, RuntimeHealth, RuntimeKind};
    use crate::session::{SessionMetadata, SessionRecord, SessionRecordInput, SessionStatus};

    use super::*;

    #[test]
    fn health_rendering_uses_shared_session_display_fallbacks() {
        let session = SessionRecord::new(SessionRecordInput {
            container_id: "container-id".to_string(),
            container_name: "broken-container".to_string(),
            container_kind: AgentboxContainerKind::Managed,
            metadata: SessionMetadata::from_labels(&BTreeMap::new()),
            attach_endpoint: None,
            container_running: true,
            status: SessionStatus::Running,
        });

        let rows = health_rows(vec![&session], &UnusedProbe);
        let table = render_table(&rows);

        assert!(table.contains("unknown"));
        assert!(table.contains("missing runtime metadata"));
        assert!(!table.contains("broken-container"));

        let json = render_json(&rows).unwrap();
        let rows: Vec<Value> = serde_json::from_str(&json).unwrap();

        assert_eq!(
            json,
            concat!(
                r#"[{"id":null,"canonical_git_root":null,"runtime":null,"health":"unhealthy","reason":"missing runtime metadata","endpoint":null,"container_name":"broken-container"}]"#,
                "\n"
            )
        );
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
