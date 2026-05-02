use comfy_table::{Cell, Table, presets::UTF8_FULL};

use crate::error::Result;
use crate::podman::Podman;
use crate::session::{SessionRecord, discover_managed_sessions};

pub fn run() -> Result<()> {
    let podman = Podman::new();
    let sessions = discover_managed_sessions(&podman)?;
    print_table(&sessions);
    Ok(())
}

pub fn print_table(sessions: &[SessionRecord]) {
    print!("{}", render_table(sessions));
}

pub fn render_table(sessions: &[SessionRecord]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(["canonical git root", "runtime", "status", "container name"]);

    let mut rows = sessions.to_vec();
    rows.sort_by(|left, right| {
        left.canonical_git_root
            .as_ref()
            .map(|root| root.as_str())
            .cmp(&right.canonical_git_root.as_ref().map(|root| root.as_str()))
            .then_with(|| left.container_name.cmp(&right.container_name))
    });

    for session in rows {
        table.add_row([
            Cell::new(
                session
                    .canonical_git_root
                    .as_ref()
                    .map_or("-", |root| root.as_str()),
            ),
            Cell::new(session.runtime.as_deref().unwrap_or("-")),
            Cell::new(session.status.as_str()),
            Cell::new(session.container_name),
        ]);
    }

    table.to_string()
}
