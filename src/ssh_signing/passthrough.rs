// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use camino::Utf8Path;

use crate::Result;
use crate::diagnostic;
use crate::git::Git;
use crate::runtime::RuntimeRunSpec;

use super::commit_signing::{self, GitIdentityPassthrough, SshCommitSigningPassthrough};
use super::known_hosts::PreparedKnownHosts;

#[derive(Debug, Default)]
pub(crate) struct SshPassthroughGuard {
    _known_hosts_file: Option<tempfile::NamedTempFile>,
}

#[derive(Debug, Default)]
struct SshKnownHostsPassthrough {
    prepared: Option<PreparedKnownHosts>,
}

impl SshKnownHostsPassthrough {
    fn apply_to(self, run_spec: &mut RuntimeRunSpec) -> SshPassthroughGuard {
        let Some(prepared) = self.prepared else {
            return SshPassthroughGuard::default();
        };
        let (mount, env, file) = prepared.into_parts();
        run_spec.add_create_mount(mount);
        run_spec.extend_create_default_env(env);
        SshPassthroughGuard {
            _known_hosts_file: Some(file),
        }
    }
}

#[derive(Debug, Default)]
struct SshPassthrough {
    commit_signing: SshCommitSigningPassthrough,
    known_hosts: SshKnownHostsPassthrough,
}

impl SshPassthrough {
    fn apply_to(self, run_spec: &mut RuntimeRunSpec) -> SshPassthroughGuard {
        self.commit_signing.apply_to(run_spec);
        self.known_hosts.apply_to(run_spec)
    }
}

pub(crate) fn apply_git_and_ssh_passthrough(
    run_spec: &mut RuntimeRunSpec,
    git_root: &Utf8Path,
    git_identity: GitIdentityPassthrough,
) -> SshPassthroughGuard {
    let git = Git::new();
    detect_with(
        git_root,
        git_identity,
        |name| std::env::var_os(name),
        |git_root, key| git.config_get(git_root, key),
        |git_root, key| git.config_path_get(git_root, key),
        |git_root, environment, warning| {
            super::known_hosts::prepare(git_root, environment, warning)
        },
        diagnostic::warning,
    )
    .apply_to(run_spec)
}

fn detect_with(
    git_root: &Utf8Path,
    git_identity: GitIdentityPassthrough,
    mut environment: impl FnMut(&str) -> Option<std::ffi::OsString>,
    mut git_config: impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    mut git_config_path: impl FnMut(&Utf8Path, &str) -> Result<Option<String>>,
    mut known_hosts: impl FnMut(
        &Utf8Path,
        &mut dyn FnMut(&str) -> Option<std::ffi::OsString>,
        &mut dyn FnMut(String),
    ) -> Option<PreparedKnownHosts>,
    mut warning: impl FnMut(String),
) -> SshPassthrough {
    let commit_signing = commit_signing::detect_with(
        git_root,
        git_identity,
        &mut environment,
        &mut git_config,
        &mut git_config_path,
        &mut warning,
    );

    let known_hosts = if commit_signing.host_agent_available {
        known_hosts(git_root, &mut environment, &mut warning)
    } else {
        None
    };

    SshPassthrough {
        commit_signing: commit_signing.passthrough,
        known_hosts: SshKnownHostsPassthrough {
            prepared: known_hosts,
        },
    }
}
