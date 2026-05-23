// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use agentbox::cli::{
    CleanArgs, Cli, Command, CompletionArgs, CompletionShell, ConnectArgs, DevEnvMode, ExecArgs,
    HealthArgs, LsArgs, OutputFormat, RestartArgs, RunArgs, RuntimeArgs, RuntimeCommand,
    RuntimeUpdateArgs, StartArgs, StopArgs,
};
use agentbox::runtime::RuntimeKind;
use assert_cmd::Command as AssertCommand;
use clap::Parser;
use predicates::prelude::*;

#[test]
fn help_lists_core_commands() {
    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();

    command.arg("--help");

    command.assert().success().stdout(
        predicate::str::contains("run")
            .and(predicate::str::contains("exec"))
            .and(predicate::str::contains("start"))
            .and(predicate::str::contains("restart"))
            .and(predicate::str::contains("runtime"))
            .and(predicate::str::contains("connect"))
            .and(predicate::str::contains("attach").not())
            .and(predicate::str::contains("ls"))
            .and(predicate::str::contains("health"))
            .and(predicate::str::contains("stop"))
            .and(predicate::str::contains("clean"))
            .and(predicate::str::contains("completion"))
            .and(predicate::str::contains("detached runtime server")),
    );
}

#[test]
fn unknown_subcommand_fails() {
    let mut command = AssertCommand::cargo_bin("agentbox").unwrap();

    command.arg("bogus");

    command
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unrecognized subcommand 'bogus'"));
}

#[test]
fn core_commands_parse_into_expected_variants() {
    let run = Cli::try_parse_from(["agentbox", "run", "--runtime", "opencode", "/tmp/workspace"])
        .unwrap();
    let exec = Cli::try_parse_from([
        "agentbox",
        "exec",
        "--dev-env",
        "none",
        "/tmp/workspace",
        "--",
        "--json",
        "fix-tests",
    ])
    .unwrap();
    let start = Cli::try_parse_from([
        "agentbox",
        "start",
        "--connect",
        "--runtime",
        "opencode",
        "/tmp/workspace",
    ])
    .unwrap();
    let restart = Cli::try_parse_from([
        "agentbox",
        "restart",
        "--connect",
        "--dev-env",
        "none",
        "abc123",
    ])
    .unwrap();
    let connect = Cli::try_parse_from(["agentbox", "connect", "/tmp/workspace"]).unwrap();
    let ls = Cli::try_parse_from(["agentbox", "ls"]).unwrap();
    let health = Cli::try_parse_from(["agentbox", "health"]).unwrap();
    let stop = Cli::try_parse_from(["agentbox", "stop", "/tmp/workspace"]).unwrap();
    let clean = Cli::try_parse_from(["agentbox", "clean"]).unwrap();
    let runtime = Cli::try_parse_from(["agentbox", "runtime", "update", "codex"]).unwrap();
    let completion = Cli::try_parse_from(["agentbox", "completion", "bash"]).unwrap();

    assert_eq!(
        run.command,
        Command::Run(RunArgs {
            runtime: Some(RuntimeKind::Opencode),
            dev_env: DevEnvMode::Auto,
            directory: "/tmp/workspace".into(),
            agent_args: Vec::new(),
        })
    );
    assert_eq!(
        connect.command,
        Command::Connect(ConnectArgs {
            directory: Some("/tmp/workspace".into()),
            agent_args: Vec::new(),
        })
    );
    assert_eq!(
        exec.command,
        Command::Exec(ExecArgs {
            dev_env: DevEnvMode::None,
            directory: "/tmp/workspace".into(),
            codex_args: vec!["--json".to_string(), "fix-tests".to_string()],
        })
    );
    assert_eq!(
        start.command,
        Command::Start(StartArgs {
            runtime: Some(RuntimeKind::Opencode),
            dev_env: DevEnvMode::Auto,
            connect: true,
            directory: "/tmp/workspace".into(),
            agent_args: Vec::new(),
        })
    );
    assert_eq!(
        restart.command,
        Command::Restart(RestartArgs {
            dev_env: DevEnvMode::None,
            connect: true,
            target: Some("abc123".into()),
        })
    );
    assert_eq!(
        ls.command,
        Command::Ls(LsArgs {
            output: OutputFormat::Table,
        })
    );
    assert_eq!(
        health.command,
        Command::Health(HealthArgs {
            output: OutputFormat::Table,
            target: None,
        })
    );
    assert_eq!(
        stop.command,
        Command::Stop(StopArgs {
            all: false,
            force: false,
            targets: vec!["/tmp/workspace".into()],
        })
    );
    assert_eq!(
        clean.command,
        Command::Clean(CleanArgs {
            dry_run: false,
            yes: false,
            images: false,
            volumes: false,
        })
    );
    assert_eq!(
        runtime.command,
        Command::Runtime(RuntimeArgs {
            command: RuntimeCommand::Update(RuntimeUpdateArgs {
                runtime: RuntimeKind::Codex,
            }),
        })
    );
    assert_eq!(
        completion.command,
        Command::Completion(CompletionArgs {
            shell: CompletionShell::Bash,
        })
    );
}

#[test]
fn exec_accepts_empty_codex_args() {
    let cli = Cli::try_parse_from(["agentbox", "exec", "/tmp/workspace"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Exec(ExecArgs {
            dev_env: DevEnvMode::Auto,
            directory: "/tmp/workspace".into(),
            codex_args: Vec::new(),
        })
    );
}

#[test]
fn exec_requires_double_dash_before_codex_args() {
    let bare_arg_error =
        Cli::try_parse_from(["agentbox", "exec", "/tmp/workspace", "fix-tests"]).unwrap_err();
    let codex_flag_error =
        Cli::try_parse_from(["agentbox", "exec", "/tmp/workspace", "--model", "gpt-5"])
            .unwrap_err();

    assert_eq!(bare_arg_error.exit_code(), 2);
    assert_eq!(codex_flag_error.exit_code(), 2);
}

#[test]
fn completion_rejects_unknown_shell() {
    let error = Cli::try_parse_from(["agentbox", "completion", "powershell"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("invalid value 'powershell'"),
        "expected clap to reject unsupported completion shells"
    );
}

#[test]
fn clean_accepts_cleanup_flags() {
    for (args, expected) in [
        (
            vec!["agentbox", "clean", "--dry-run"],
            CleanArgs {
                dry_run: true,
                yes: false,
                images: false,
                volumes: false,
            },
        ),
        (
            vec!["agentbox", "clean", "--yes"],
            CleanArgs {
                dry_run: false,
                yes: true,
                images: false,
                volumes: false,
            },
        ),
        (
            vec!["agentbox", "clean", "--images"],
            CleanArgs {
                dry_run: false,
                yes: false,
                images: true,
                volumes: false,
            },
        ),
        (
            vec!["agentbox", "clean", "--volumes"],
            CleanArgs {
                dry_run: false,
                yes: false,
                images: false,
                volumes: true,
            },
        ),
    ] {
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.command, Command::Clean(expected));
    }
}

#[test]
fn clean_rejects_dry_run_with_yes() {
    let error = Cli::try_parse_from(["agentbox", "clean", "--dry-run", "--yes"]).unwrap_err();

    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn global_verbose_flag_is_available_before_or_after_subcommands() {
    let before = Cli::try_parse_from([
        "agentbox",
        "--verbose",
        "run",
        "--runtime",
        "opencode",
        "/tmp/workspace",
    ])
    .unwrap();
    let after = Cli::try_parse_from([
        "agentbox",
        "run",
        "--runtime",
        "opencode",
        "--verbose",
        "/tmp/workspace",
    ])
    .unwrap();

    assert!(before.verbose);
    assert!(after.verbose);
}

#[test]
fn run_accepts_dev_env_modes_and_defaults_to_auto() {
    let defaulted =
        Cli::try_parse_from(["agentbox", "run", "--runtime", "opencode", "/tmp/workspace"])
            .unwrap();
    let none = Cli::try_parse_from([
        "agentbox",
        "run",
        "--runtime",
        "opencode",
        "--dev-env",
        "none",
        "/tmp/workspace",
    ])
    .unwrap();

    assert!(matches!(
        defaulted.command,
        Command::Run(RunArgs {
            dev_env: DevEnvMode::Auto,
            ..
        })
    ));
    assert!(matches!(
        none.command,
        Command::Run(RunArgs {
            dev_env: DevEnvMode::None,
            ..
        })
    ));
}

#[test]
fn run_rejects_unknown_dev_env_mode() {
    let error = Cli::try_parse_from([
        "agentbox",
        "run",
        "--runtime",
        "opencode",
        "--dev-env",
        "shell",
        "/tmp/workspace",
    ])
    .unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("invalid value 'shell'"),
        "expected clap to reject unsupported dev-env modes"
    );
}

#[test]
fn stop_accepts_force_cleanup_flag() {
    let cli = Cli::try_parse_from(["agentbox", "stop", "--force", "/tmp/workspace"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Stop(StopArgs {
            all: false,
            force: true,
            targets: vec!["/tmp/workspace".into()],
        })
    );
}

#[test]
fn stop_accepts_multiple_targets() {
    let cli =
        Cli::try_parse_from(["agentbox", "stop", "/tmp/first", "abc123", "/tmp/second"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Stop(StopArgs {
            all: false,
            force: false,
            targets: vec!["/tmp/first".into(), "abc123".into(), "/tmp/second".into()],
        })
    );
}

#[test]
fn stop_accepts_all_without_target() {
    let cli = Cli::try_parse_from(["agentbox", "stop", "--all"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Stop(StopArgs {
            all: true,
            force: false,
            targets: Vec::new(),
        })
    );
}

#[test]
fn stop_rejects_all_with_target() {
    let error = Cli::try_parse_from(["agentbox", "stop", "--all", "/tmp/workspace"]).unwrap_err();

    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn ls_accepts_output_format_selection() {
    for args in [
        vec!["agentbox", "ls", "--output", "json"],
        vec!["agentbox", "ls", "--output=json"],
        vec!["agentbox", "ls", "-o", "json"],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(
            cli.command,
            Command::Ls(LsArgs {
                output: OutputFormat::Json,
            })
        );
    }

    let cli = Cli::try_parse_from(["agentbox", "ls", "--output", "table"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Ls(LsArgs {
            output: OutputFormat::Table,
        })
    );
}

#[test]
fn health_accepts_output_format_selection() {
    for args in [
        vec!["agentbox", "health", "--output", "json"],
        vec!["agentbox", "health", "--output=json"],
        vec!["agentbox", "health", "-o", "json"],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(
            cli.command,
            Command::Health(HealthArgs {
                output: OutputFormat::Json,
                target: None,
            })
        );
    }

    let cli = Cli::try_parse_from(["agentbox", "health", "--output", "table"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Health(HealthArgs {
            output: OutputFormat::Table,
            target: None,
        })
    );
}

#[test]
fn health_accepts_stable_id_target_with_output_selection() {
    let cli = Cli::try_parse_from(["agentbox", "health", "--output", "json", "abc123"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Health(HealthArgs {
            output: OutputFormat::Json,
            target: Some("abc123".to_string()),
        })
    );
}

#[test]
fn output_format_rejects_unknown_values() {
    for args in [
        vec!["agentbox", "ls", "--output", "yaml"],
        vec!["agentbox", "health", "-o", "yaml"],
    ] {
        let error = Cli::try_parse_from(args).unwrap_err();

        assert_eq!(error.exit_code(), 2);
        assert!(
            error.to_string().contains("invalid value 'yaml'"),
            "expected clap to reject the unsupported output format"
        );
    }
}

#[test]
fn directory_commands_require_a_path_argument() {
    let error = Cli::try_parse_from(["agentbox", "run", "--runtime", "opencode"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("<DIRECTORY>"),
        "expected clap to mention the missing directory argument"
    );
}

#[test]
fn run_rejects_image_override() {
    let error = Cli::try_parse_from([
        "agentbox",
        "run",
        "--runtime",
        "opencode",
        "--image",
        "registry.example/agentbox:test",
        "/tmp/workspace",
    ])
    .unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error.to_string().contains("unexpected argument '--image'"),
        "expected clap to reject the removed image option"
    );
}

#[test]
fn run_accepts_runtime_selection() {
    let cli =
        Cli::try_parse_from(["agentbox", "run", "--runtime", "codex", "/tmp/workspace"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Run(RunArgs {
            runtime: Some(RuntimeKind::Codex),
            dev_env: DevEnvMode::Auto,
            directory: "/tmp/workspace".into(),
            agent_args: Vec::new(),
        })
    );
}

#[test]
fn run_accepts_agent_client_args_after_double_dash() {
    let cli = Cli::try_parse_from([
        "agentbox",
        "run",
        "--runtime",
        "codex",
        "/tmp/workspace",
        "--",
        "--no-alt-screen",
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        Command::Run(RunArgs {
            runtime: Some(RuntimeKind::Codex),
            dev_env: DevEnvMode::Auto,
            directory: "/tmp/workspace".into(),
            agent_args: vec!["--no-alt-screen".to_string()],
        })
    );
}

#[test]
fn start_accepts_connect_flag() {
    for flag in ["--connect", "-c"] {
        let cli = Cli::try_parse_from([
            "agentbox",
            "start",
            flag,
            "--runtime",
            "codex",
            "/tmp/workspace",
        ])
        .unwrap();

        assert_eq!(
            cli.command,
            Command::Start(StartArgs {
                runtime: Some(RuntimeKind::Codex),
                dev_env: DevEnvMode::Auto,
                connect: true,
                directory: "/tmp/workspace".into(),
                agent_args: Vec::new(),
            })
        );
    }
}

#[test]
fn start_accepts_agent_server_args_after_double_dash() {
    let cli = Cli::try_parse_from([
        "agentbox",
        "start",
        "--runtime",
        "opencode",
        "/tmp/workspace",
        "--",
        "--server-flag",
        "value",
    ])
    .unwrap();

    assert_eq!(
        cli.command,
        Command::Start(StartArgs {
            runtime: Some(RuntimeKind::Opencode),
            dev_env: DevEnvMode::Auto,
            connect: false,
            directory: "/tmp/workspace".into(),
            agent_args: vec!["--server-flag".to_string(), "value".to_string()],
        })
    );
}

#[test]
fn restart_accepts_optional_target_connect_and_dev_env() {
    let defaulted = Cli::try_parse_from(["agentbox", "restart"]).unwrap();
    assert_eq!(
        defaulted.command,
        Command::Restart(RestartArgs {
            dev_env: DevEnvMode::Auto,
            connect: false,
            target: None,
        })
    );

    for flag in ["--connect", "-c"] {
        let cli = Cli::try_parse_from([
            "agentbox",
            "restart",
            flag,
            "--dev-env",
            "none",
            "/tmp/workspace",
        ])
        .unwrap();

        assert_eq!(
            cli.command,
            Command::Restart(RestartArgs {
                dev_env: DevEnvMode::None,
                connect: true,
                target: Some("/tmp/workspace".into()),
            })
        );
    }
}

#[test]
fn restart_rejects_unsupported_flags() {
    for args in [
        vec!["agentbox", "restart", "--runtime", "opencode", "abc123"],
        vec!["agentbox", "restart", "--all"],
        vec!["agentbox", "restart", "--force", "abc123"],
    ] {
        let error = Cli::try_parse_from(args).unwrap_err();

        assert_eq!(error.exit_code(), 2);
        assert!(
            error.to_string().contains("unexpected argument"),
            "expected clap to reject unsupported restart flag"
        );
    }
}

#[test]
fn run_rejects_connect_flag() {
    for flag in ["--connect", "-c"] {
        let error = Cli::try_parse_from([
            "agentbox",
            "run",
            flag,
            "--runtime",
            "codex",
            "/tmp/workspace",
        ])
        .unwrap_err();

        assert_eq!(error.exit_code(), 2);
        assert!(
            error.to_string().contains("unexpected argument"),
            "expected clap to reject {flag} for run"
        );
    }
}

#[test]
fn agent_passthrough_flags_require_double_dash() {
    for args in [
        vec![
            "agentbox",
            "run",
            "--runtime",
            "codex",
            "/tmp/workspace",
            "--no-alt-screen",
        ],
        vec![
            "agentbox",
            "start",
            "--runtime",
            "opencode",
            "/tmp/workspace",
            "--server-flag",
        ],
        vec!["agentbox", "connect", "/tmp/workspace", "--no-alt-screen"],
    ] {
        let error = Cli::try_parse_from(args).unwrap_err();

        assert_eq!(error.exit_code(), 2);
        assert!(
            error.to_string().contains("unexpected argument"),
            "expected clap to require `--` before agent passthrough flags"
        );
    }
}

#[test]
fn run_accepts_missing_runtime_for_prompting() {
    let cli = Cli::try_parse_from(["agentbox", "run", "/tmp/workspace"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Run(RunArgs {
            runtime: None,
            dev_env: DevEnvMode::Auto,
            directory: "/tmp/workspace".into(),
            agent_args: Vec::new(),
        })
    );
}

#[test]
fn connect_accepts_missing_directory_for_prompting() {
    let cli = Cli::try_parse_from(["agentbox", "connect"]).unwrap();

    assert_eq!(
        cli.command,
        Command::Connect(ConnectArgs {
            directory: None,
            agent_args: Vec::new(),
        })
    );
}

#[test]
fn connect_accepts_agent_client_args_after_double_dash() {
    let with_directory = Cli::try_parse_from([
        "agentbox",
        "connect",
        "/tmp/workspace",
        "--",
        "--no-alt-screen",
    ])
    .unwrap();
    let with_prompt =
        Cli::try_parse_from(["agentbox", "connect", "--", "--no-alt-screen"]).unwrap();

    assert_eq!(
        with_directory.command,
        Command::Connect(ConnectArgs {
            directory: Some("/tmp/workspace".into()),
            agent_args: vec!["--no-alt-screen".to_string()],
        })
    );
    assert_eq!(
        with_prompt.command,
        Command::Connect(ConnectArgs {
            directory: None,
            agent_args: vec!["--no-alt-screen".to_string()],
        })
    );
}

#[test]
fn connect_rejects_runtime_and_image_flags() {
    let runtime_error = Cli::try_parse_from([
        "agentbox",
        "connect",
        "--runtime",
        "codex",
        "/tmp/workspace",
    ])
    .unwrap_err();
    assert_eq!(runtime_error.exit_code(), 2);
    assert!(
        runtime_error
            .to_string()
            .contains("unexpected argument '--runtime'")
    );

    let image_error = Cli::try_parse_from([
        "agentbox",
        "connect",
        "--image",
        "example:test",
        "/tmp/workspace",
    ])
    .unwrap_err();
    assert_eq!(image_error.exit_code(), 2);
    assert!(
        image_error
            .to_string()
            .contains("unexpected argument '--image'")
    );
}

#[test]
fn attach_is_not_a_supported_subcommand() {
    let error = Cli::try_parse_from(["agentbox", "attach"]).unwrap_err();

    assert_eq!(error.exit_code(), 2);
    assert!(
        error
            .to_string()
            .contains("unrecognized subcommand 'attach'")
    );
}
