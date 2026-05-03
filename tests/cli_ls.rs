use agentbox::cli::{Cli, Command};
use agentbox::commands::ls::render_table;
use agentbox::session::{SessionMetadata, SessionRecord, SessionStatus};
use camino::Utf8PathBuf;
use clap::Parser;

#[test]
fn ls_renders_all_status_rows_in_stable_order() {
    let sessions = vec![
        session("/workspace/b", "beta", SessionStatus::Failed),
        session("/workspace/a", "alpha-one", SessionStatus::Running),
        session("/workspace/c", "gamma", SessionStatus::Orphaned),
        session("/workspace/d", "delta", SessionStatus::Failed),
        session("/workspace/a", "alpha-two", SessionStatus::Duplicate),
    ];

    let table = render_table(&sessions);

    let alpha = line_index(&table, "alpha-one");
    let beta = line_index(&table, "beta");
    let gamma = line_index(&table, "gamma");
    let delta = line_index(&table, "delta");
    let alpha_dup = line_index(&table, "alpha-two");

    assert!(
        alpha < alpha_dup,
        "duplicate rows stay grouped by root then name"
    );
    assert!(
        alpha < beta && beta < gamma && gamma < delta,
        "roots sort lexicographically"
    );
    assert!(table.contains("running"));
    assert!(table.contains("orphaned"));
    assert!(table.contains("failed"));
    assert!(table.contains("duplicate"));
}

#[test]
fn ls_has_no_machine_readable_mode() {
    let error = Cli::try_parse_from(["agentbox", "ls", "--json"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(error.to_string().contains("unexpected argument '--json'"));
    assert!(matches!(
        Cli::try_parse_from(["agentbox", "ls"]).unwrap().command,
        Command::Ls
    ));
}

fn session(root: &str, name: &str, status: SessionStatus) -> SessionRecord {
    SessionRecord {
        container_id: format!("{name}-id"),
        container_name: name.to_string(),
        metadata: SessionMetadata {
            managed: Some("true".to_string()),
            schema: Some("1".to_string()),
            canonical_git_root: Some(Utf8PathBuf::from(root)),
            git_root_hash: Some("hash".to_string()),
            runtime: Some("opencode".to_string()),
            image: Some("image".to_string()),
            logical_name: Some(name.to_string()),
            attach_scheme: Some("http".to_string()),
            container_port: Some("4096".to_string()),
            container_listen_ip: Some("0.0.0.0".to_string()),
        },
        attach_endpoint: None,
        failure: None,
        status,
    }
}

fn line_index(table: &str, needle: &str) -> usize {
    table
        .lines()
        .position(|line| line.contains(needle))
        .unwrap()
}
