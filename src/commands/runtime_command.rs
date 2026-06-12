// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::env;
use std::net::IpAddr;
use std::process::{ExitStatus, Stdio};

use camino::Utf8Path;

use crate::dev_env::DevEnvironment;
use crate::process::{ProcessRunner, format_status};
use crate::runtime::{
    AttachEndpoint, CODEX_REMOTE_TOKEN_ENV, DEFAULT_HOST_ATTACH_IP, RuntimeInvocation, RuntimeKind,
};
use crate::{Error, Result};

use super::codex_attach_auth::CodexAttachToken;

pub(crate) fn server_runtime_command(
    runtime: RuntimeKind,
    target: &Utf8Path,
    dev_env: &DevEnvironment,
    codex_attach_token: Option<&CodexAttachToken>,
    server_args: &[String],
) -> Result<RuntimeInvocation> {
    let mut argv = runtime.server_command().argv;
    if runtime == RuntimeKind::Codex {
        let token = codex_attach_token
            .ok_or_else(|| Error::msg("missing Codex attach token for runtime server command"))?;
        argv.extend([
            "--ws-auth".to_string(),
            "capability-token".to_string(),
            "--ws-token-sha256".to_string(),
            token.sha256().to_string(),
        ]);
    }
    argv.extend(server_args.iter().cloned());

    Ok(RuntimeInvocation::new(
        dev_env.wrap_argv(argv),
        target.to_path_buf(),
    ))
}

pub(crate) fn codex_exec_runtime_command(
    target: &Utf8Path,
    dev_env: &DevEnvironment,
    codex_args: Vec<String>,
) -> RuntimeInvocation {
    let mut argv = RuntimeKind::Codex.foreground_command().argv;
    argv.push("exec".to_string());
    argv.extend(["--disable".to_string(), "codex_git_commit".to_string()]);
    argv.extend(codex_args);

    RuntimeInvocation::new(dev_env.wrap_argv(argv), target.to_path_buf())
}

fn host_client_runtime_command(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
    client_args: &[String],
) -> RuntimeInvocation {
    let mut argv = runtime.host_client_command(endpoint).argv;
    argv.extend(client_args.iter().cloned());

    RuntimeInvocation::new(argv, launch_directory.to_path_buf())
}

pub(crate) fn run_host_runtime_client(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
    codex_attach_token: Option<&CodexAttachToken>,
    client_args: &[String],
) -> Result<()> {
    let status = run_host_runtime_client_status(
        runtime,
        endpoint,
        launch_directory,
        codex_attach_token,
        client_args,
    )?;
    if status.success() {
        Ok(())
    } else {
        Err(host_client_status_error(
            runtime,
            endpoint,
            launch_directory,
            status,
            client_args,
        ))
    }
}

pub(crate) fn run_host_runtime_client_status(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
    codex_attach_token: Option<&CodexAttachToken>,
    client_args: &[String],
) -> Result<ExitStatus> {
    let process_runner = ProcessRunner::new();
    let client = host_client_runtime_command(runtime, endpoint, launch_directory, client_args);
    let codex_attach_token = required_codex_client_token(runtime, codex_attach_token)?;
    let no_proxy = loopback_no_proxy_value_from_env(endpoint);

    run_host_client(&process_runner, &client, codex_attach_token, no_proxy)
}

pub(crate) fn ensure_host_runtime_client_available(runtime: RuntimeKind) -> Result<()> {
    let attach = runtime.attach_spec();
    let endpoint = AttachEndpoint {
        scheme: attach.scheme.to_string(),
        host_ip: DEFAULT_HOST_ATTACH_IP.to_string(),
        host_port: attach.container_port,
    };
    let command = runtime.host_client_command(&endpoint);
    let Some(program) = command.argv.first() else {
        return Err(Error::msg("runtime host client command is empty"));
    };

    which::which(program)
        .map(|_| ())
        .map_err(|_| Error::msg(host_client_not_found_message(program)))
}

pub(crate) fn host_client_status_error(
    runtime: RuntimeKind,
    endpoint: &AttachEndpoint,
    launch_directory: &Utf8Path,
    status: ExitStatus,
    client_args: &[String],
) -> Error {
    let client = host_client_runtime_command(runtime, endpoint, launch_directory, client_args);
    Error::msg(format!(
        "`{}` exited with {}",
        client.argv().join(" "),
        format_status(status)
    ))
}

fn run_host_client(
    process_runner: &ProcessRunner,
    client: &RuntimeInvocation,
    codex_attach_token: Option<&CodexAttachToken>,
    no_proxy: Option<String>,
) -> Result<ExitStatus> {
    let argv = client.argv();
    let Some((program, args)) = argv.split_first() else {
        return Err(Error::msg("runtime host client command is empty"));
    };

    process_runner
        .configured_command(program, |command| {
            command.args(args);
            command.current_dir(client.workdir().as_std_path());
            if let Some(token) = codex_attach_token {
                command.env(CODEX_REMOTE_TOKEN_ENV, token.value());
            }
            if let Some(no_proxy) = no_proxy.as_deref() {
                command.env("NO_PROXY", no_proxy);
                command.env("no_proxy", no_proxy);
            }
            command.stdin(Stdio::inherit());
            command.stdout(Stdio::inherit());
            command.stderr(Stdio::inherit());
        })?
        .status()
}

fn required_codex_client_token(
    runtime: RuntimeKind,
    token: Option<&CodexAttachToken>,
) -> Result<Option<&CodexAttachToken>> {
    if runtime == RuntimeKind::Codex {
        token
            .map(Some)
            .ok_or_else(|| Error::msg("missing Codex attach token for host client command"))
    } else {
        Ok(None)
    }
}

fn host_client_not_found_message(program: &str) -> String {
    format!("`{program}` was not found on PATH; install `{program}` or add it to PATH")
}

fn loopback_no_proxy_value_from_env(endpoint: &AttachEndpoint) -> Option<String> {
    merge_loopback_no_proxy(
        env::var("NO_PROXY").ok().as_deref(),
        env::var("no_proxy").ok().as_deref(),
        endpoint.host_ip.as_str(),
    )
}

fn merge_loopback_no_proxy(
    upper: Option<&str>,
    lower: Option<&str>,
    endpoint_host: &str,
) -> Option<String> {
    if !is_loopback_host(endpoint_host) {
        return None;
    }

    let mut entries = Vec::new();
    for value in upper.into_iter().chain(lower) {
        for entry in value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            push_unique(&mut entries, entry);
        }
    }

    for entry in ["127.0.0.1", "localhost", "::1", endpoint_host] {
        push_unique(&mut entries, entry);
    }

    Some(entries.join(","))
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    host.parse::<IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

fn push_unique(entries: &mut Vec<String>, entry: &str) {
    if !entries.iter().any(|existing| existing == entry) {
        entries.push(entry.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::merge_loopback_no_proxy;

    #[test]
    fn loopback_no_proxy_defaults_cover_standard_loopback_hosts() {
        assert_eq!(
            merge_loopback_no_proxy(None, None, "127.0.0.1").as_deref(),
            Some("127.0.0.1,localhost,::1")
        );
    }

    #[test]
    fn loopback_no_proxy_includes_distinct_endpoint_host() {
        assert_eq!(
            merge_loopback_no_proxy(None, None, "127.0.0.2").as_deref(),
            Some("127.0.0.1,localhost,::1,127.0.0.2")
        );
    }

    #[test]
    fn loopback_no_proxy_preserves_existing_entries_without_exact_duplicates() {
        assert_eq!(
            merge_loopback_no_proxy(
                Some("example.test, localhost ,127.0.0.1"),
                Some("internal.test,example.test"),
                "localhost",
            )
            .as_deref(),
            Some("example.test,localhost,127.0.0.1,internal.test,::1")
        );
    }

    #[test]
    fn loopback_no_proxy_detects_localhost_and_loopback_ips() {
        assert_eq!(
            merge_loopback_no_proxy(None, None, "LOCALHOST").as_deref(),
            Some("127.0.0.1,localhost,::1,LOCALHOST")
        );
        assert_eq!(
            merge_loopback_no_proxy(None, None, "::1").as_deref(),
            Some("127.0.0.1,localhost,::1")
        );
    }

    #[test]
    fn non_loopback_no_proxy_is_unchanged() {
        assert_eq!(
            merge_loopback_no_proxy(Some("example.test"), Some("internal.test"), "192.0.2.10"),
            None
        );
    }
}
