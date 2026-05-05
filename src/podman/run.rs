// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::runtime::{RuntimeCreateSpec, RuntimeMount};

use super::args::PodmanArgs;

pub(super) fn run_detached_args(
    container_name: &str,
    spec: &RuntimeCreateSpec,
    workdir: Option<&str>,
) -> Vec<String> {
    let mut args = PodmanArgs::from(["run", "--detach", "--rm"]);
    args.option("--name", container_name);
    args.option("--userns", "keep-id:uid=1000,gid=1000");

    if let Some(workdir) = workdir {
        args.option("--workdir", workdir);
    }

    for (name, value) in &spec.labels {
        args.key_value_option("--label", name, value);
    }

    for mount in &spec.mounts {
        args.option("--mount", render_mount(mount));
    }

    for (name, value) in &spec.default_env {
        args.key_value_option("--env", name, value);
    }

    if !spec.network_enabled {
        args.flag("--network=none");
    }

    for port in &spec.published_ports {
        args.option("--publish", port);
    }

    args.flag(spec.image.as_str());
    args.extend(spec.command.iter().map(String::as_str));
    args.into_vec()
}

fn render_mount(mount: &RuntimeMount) -> String {
    let mut options = vec![
        format!("type={}", mount.kind.podman_type()),
        format!("src={}", mount.source),
        format!("dst={}", mount.destination),
    ];
    if mount.read_only {
        options.push("ro".to_string());
    }
    if mount.kind == crate::runtime::RuntimeMountKind::Volume {
        options.push("U".to_string());
    }
    options.join(",")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::podman::args::strings;
    use crate::runtime::RuntimeMount;

    #[test]
    fn run_detached_args_are_stable_and_complete() {
        let spec = RuntimeCreateSpec {
            image: "localhost/agentbox-opencode:local".to_string(),
            labels: BTreeMap::from([
                ("io.agentbox.managed".to_string(), "true".to_string()),
                ("io.agentbox.runtime".to_string(), "opencode".to_string()),
            ]),
            mounts: vec![
                RuntimeMount::read_only_bind("/workspace", "/workspace"),
                RuntimeMount::volume("agentbox-cache", "/home/user/.cache/nix"),
            ],
            command: strings([
                "opencode",
                "serve",
                "--hostname",
                "0.0.0.0",
                "--port",
                "4096",
            ]),
            default_env: BTreeMap::from([(
                "NIX_CONFIG".to_string(),
                "sandbox = false".to_string(),
            )]),
            network_enabled: false,
            published_ports: vec!["127.0.0.1::4096".to_string()],
        };

        assert_eq!(
            run_detached_args("agentbox-demo", &spec, Some("/workspace")),
            strings([
                "run",
                "--detach",
                "--rm",
                "--name",
                "agentbox-demo",
                "--userns",
                "keep-id:uid=1000,gid=1000",
                "--workdir",
                "/workspace",
                "--label",
                "io.agentbox.managed=true",
                "--label",
                "io.agentbox.runtime=opencode",
                "--mount",
                "type=bind,src=/workspace,dst=/workspace,ro",
                "--mount",
                "type=volume,src=agentbox-cache,dst=/home/user/.cache/nix,U",
                "--env",
                "NIX_CONFIG=sandbox = false",
                "--network=none",
                "--publish",
                "127.0.0.1::4096",
                "localhost/agentbox-opencode:local",
                "opencode",
                "serve",
                "--hostname",
                "0.0.0.0",
                "--port",
                "4096",
            ])
        );
    }
}
