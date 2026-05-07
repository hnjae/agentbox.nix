use comfy_table::Cell;
use serde::Serialize;

use crate::cli::{LsArgs, OutputFormat};
use crate::error::Result;
use crate::podman::Podman;
use crate::session::{SessionRecord, discover_managed_sessions, sorted_session_refs_by_identity};

use super::output;

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
    let mut table = output::table(["ID", "CANONICAL GIT ROOT", "RUNTIME", "STATUS", "ENDPOINT"]);

    for session in sorted_session_refs_by_identity(sessions) {
        let display = session.display();
        table.add_row([
            Cell::new(display.id_or_unknown()),
            Cell::new(display.canonical_git_root_or_unknown()),
            Cell::new(display.runtime_or_unknown()),
            Cell::new(session.status.as_str()),
            Cell::new(display.endpoint_or_unknown()),
        ]);
    }

    output::render_table(table)
}

pub fn render_json(sessions: &[SessionRecord]) -> Result<String> {
    let rows = sorted_session_refs_by_identity(sessions)
        .into_iter()
        .map(LsJsonRow::from)
        .collect::<Vec<_>>();

    output::render_json(&rows)
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
        let display = session.display();

        Self {
            id: display.id(),
            canonical_git_root: display.canonical_git_root_str(),
            runtime: display.runtime(),
            status: session.status.as_str(),
            endpoint: display.endpoint().map(ToString::to_string),
            container_name: display.container_name(),
        }
    }
}
