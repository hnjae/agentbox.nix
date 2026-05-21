// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::Args;
use comfy_table::Cell;
use serde::Serialize;

use crate::Error;
use crate::error::Result;
use crate::podman::Podman;
use crate::runtime::{HostRuntimeHealthProbe, RuntimeHealth, RuntimeHealthProbe};
use crate::session::{
    SessionDiscoveryQuery, SessionRecord, select_stable_id_prefix, sort_session_refs_by_identity,
};

use super::output::{self, OutputFormat};
use super::session_output::{
    SessionDisplay, SessionJsonFields, SessionJsonLeadingFields, SessionJsonTrailingFields,
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
        table.add_row([
            Cell::new(row.display.id_or_unknown()),
            Cell::new(row.display.canonical_git_root_or_unknown()),
            Cell::new(row.display.runtime_or_unknown()),
            Cell::new(row.health.status_str()),
            Cell::new(row.health.reason()),
            Cell::new(row.display.endpoint_or_unknown()),
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
    let Some(target) = target else {
        return Ok(sessions
            .iter()
            .filter(|session| session.is_running())
            .collect());
    };

    let selection = select_stable_id_prefix(sessions, target)?;
    let selection_id = selection.id().to_string();
    let Some(session) = selection.into_single_session() else {
        return Err(Error::msg(format!(
            "stable id `{selection_id}` matches multiple managed sessions; health requires a single running session",
        )));
    };
    if !session.is_running() {
        return Err(Error::msg(format!(
            "managed session `{}` is `{}`; health only probes running sessions",
            session.stable_id().unwrap_or(&selection_id),
            session.status().as_str()
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
    leading: SessionJsonLeadingFields<'identity>,
    health: &'static str,
    reason: &'row str,
    #[serde(flatten)]
    trailing: SessionJsonTrailingFields<'identity>,
}

impl<'row, 'session> From<&'row HealthRow<'session>> for HealthJsonRow<'session, 'row> {
    fn from(row: &'row HealthRow<'session>) -> Self {
        let fields = SessionJsonFields::from_display(&row.display);

        Self {
            leading: fields.leading,
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
    use crate::session::{SessionMetadata, SessionRecord, SessionStatus};

    use super::*;

    #[test]
    fn health_rendering_uses_shared_session_display_fallbacks() {
        let session = SessionRecord::new(
            "container-id",
            "broken-container",
            AgentboxContainerKind::Managed,
            SessionMetadata::from_labels(&BTreeMap::new()),
            None,
            true,
            SessionStatus::Running,
        );

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
