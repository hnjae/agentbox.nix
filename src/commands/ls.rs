use comfy_table::{Cell, Table, presets::NOTHING};
use serde::Serialize;

use crate::cli::{LsArgs, OutputFormat};
use crate::error::Result;
use crate::podman::Podman;
use crate::session::{SessionRecord, discover_managed_sessions};

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
    table.set_header(["canonical git root", "runtime", "status", "container name"]);

    for session in sorted_sessions(sessions) {
        table.add_row([
            Cell::new(
                session
                    .canonical_git_root()
                    .map_or("unknown", |root| root.as_str()),
            ),
            Cell::new(session.runtime().unwrap_or("unknown")),
            Cell::new(session.status.as_str()),
            Cell::new(&session.container_name),
        ]);
    }

    table.to_string()
}

pub fn render_json(sessions: &[SessionRecord]) -> Result<String> {
    let rows = sorted_sessions(sessions)
        .into_iter()
        .map(LsJsonRow::from)
        .collect::<Vec<_>>();

    Ok(format!("{}\n", serde_json::to_string(&rows)?))
}

fn sorted_sessions(sessions: &[SessionRecord]) -> Vec<&SessionRecord> {
    let mut rows = sessions.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.canonical_git_root()
            .map(|root| root.as_str())
            .cmp(&right.canonical_git_root().map(|root| root.as_str()))
            .then_with(|| left.container_name.cmp(&right.container_name))
    });
    rows
}

#[derive(Debug, Serialize)]
struct LsJsonRow<'a> {
    canonical_git_root: Option<&'a str>,
    runtime: Option<&'a str>,
    status: &'static str,
    container_name: &'a str,
}

impl<'a> From<&'a SessionRecord> for LsJsonRow<'a> {
    fn from(session: &'a SessionRecord) -> Self {
        Self {
            canonical_git_root: session.canonical_git_root().map(|root| root.as_str()),
            runtime: session.runtime(),
            status: session.status.as_str(),
            container_name: &session.container_name,
        }
    }
}
