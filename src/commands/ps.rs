// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::Args;
use comfy_table::Cell;
use serde::Serialize;

use crate::error::Result;
use crate::podman::Podman;
use crate::session::{SessionDiscoveryQuery, SessionRecord, sorted_session_refs_by_identity};

use super::output::{self, OutputFormat};
use super::session_output::{
    SessionJsonFields, SessionJsonIdField, SessionJsonMetadataFields, SessionJsonTrailingFields,
    SessionTableFields,
};

#[derive(Debug, Args, PartialEq, Eq)]
pub struct PsArgs {
    /// Output format.
    #[arg(short = 'o', long = "output", value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,
}

pub fn run(args: PsArgs) -> Result<()> {
    let podman = Podman::new();
    let sessions = SessionDiscoveryQuery::agentbox_containers().discover(&podman)?;
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
    let mut table = output::table([
        "ID",
        "TYPE",
        "CANONICAL GIT ROOT",
        "RUNTIME",
        "STATUS",
        "ENDPOINT",
    ]);

    for session in sorted_session_refs_by_identity(sessions) {
        let fields = SessionTableFields::from_session(session);
        table.add_row([
            Cell::new(fields.id),
            Cell::new(session.container_kind().output_type()),
            Cell::new(fields.canonical_git_root),
            Cell::new(fields.runtime),
            Cell::new(session.status().as_str()),
            Cell::new(fields.endpoint),
        ]);
    }

    output::render_table(table)
}

pub fn render_json(sessions: &[SessionRecord]) -> Result<String> {
    let rows = sorted_session_refs_by_identity(sessions)
        .into_iter()
        .map(PsJsonRow::from)
        .collect::<Vec<_>>();

    output::render_json(&rows)
}

#[derive(Debug, Serialize)]
struct PsJsonRow<'a> {
    #[serde(flatten)]
    id: SessionJsonIdField<'a>,
    #[serde(rename = "type")]
    container_type: &'static str,
    #[serde(flatten)]
    metadata: SessionJsonMetadataFields<'a>,
    status: &'static str,
    #[serde(flatten)]
    trailing: SessionJsonTrailingFields<'a>,
}

impl<'a> From<&'a SessionRecord> for PsJsonRow<'a> {
    fn from(session: &'a SessionRecord) -> Self {
        let fields = SessionJsonFields::from_session(session);

        Self {
            id: fields.id,
            container_type: session.container_kind().output_type(),
            metadata: fields.metadata,
            status: session.status().as_str(),
            trailing: fields.trailing,
        }
    }
}
