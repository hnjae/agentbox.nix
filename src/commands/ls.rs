use comfy_table::{Cell, Table, presets::NOTHING};
use serde::Serialize;

use crate::cli::{LsArgs, OutputFormat};
use crate::error::Result;
use crate::podman::Podman;
use crate::session::{SessionRecord, discover_managed_sessions, sorted_session_refs_by_identity};

use super::table;

pub fn run(args: LsArgs) -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_managed_sessions(&podman)?;
    match args.output {
        OutputFormat::Table => print_table(&sessions),
        OutputFormat::Json => print_json(&sessions)?,
    }
    Ok(())
}

pub fn print_table(sessions: &[SessionRecord]) {
    print!("{}", render_table(sessions));
}

pub fn print_json(sessions: &[SessionRecord]) -> Result<()> {
    print!("{}", render_json(sessions)?);
    Ok(())
}

pub fn render_table(sessions: &[SessionRecord]) -> String {
    let mut table = Table::new();
    table.load_preset(NOTHING);
    table.set_header(["ID", "CANONICAL GIT ROOT", "RUNTIME", "STATUS", "ENDPOINT"]);

    for session in sorted_session_refs_by_identity(sessions) {
        table.add_row([
            Cell::new(session.stable_id().unwrap_or("unknown")),
            Cell::new(
                session
                    .canonical_git_root()
                    .map_or("unknown", |root| root.as_str()),
            ),
            Cell::new(session.runtime().unwrap_or("unknown")),
            Cell::new(session.status.as_str()),
            Cell::new(
                session
                    .attach_endpoint
                    .as_ref()
                    .map_or_else(|| "unknown".to_string(), ToString::to_string),
            ),
        ]);
    }

    table::render_table(table)
}

pub fn render_json(sessions: &[SessionRecord]) -> Result<String> {
    let rows = sorted_session_refs_by_identity(sessions)
        .into_iter()
        .map(LsJsonRow::from)
        .collect::<Vec<_>>();

    Ok(format!("{}\n", serde_json::to_string(&rows)?))
}

#[derive(Debug, Serialize)]
struct LsJsonRow<'a> {
    id: Option<&'a str>,
    canonical_git_root: Option<&'a str>,
    runtime: Option<&'a str>,
    status: &'static str,
    endpoint: Option<String>,
    container_name: &'a str,
}

impl<'a> From<&'a SessionRecord> for LsJsonRow<'a> {
    fn from(session: &'a SessionRecord) -> Self {
        Self {
            id: session.stable_id(),
            canonical_git_root: session.canonical_git_root().map(|root| root.as_str()),
            runtime: session.runtime(),
            status: session.status.as_str(),
            endpoint: session.attach_endpoint.as_ref().map(ToString::to_string),
            container_name: &session.container_name,
        }
    }
}
