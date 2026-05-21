// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use camino::Utf8Path;

use super::args::PodmanArgs;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PodmanBuildOptions {
    pub build_args: BTreeMap<String, String>,
    pub labels: BTreeMap<String, String>,
}

pub(super) fn build_image_args(
    image: &str,
    containerfile: &Utf8Path,
    context_dir: &Utf8Path,
    options: &PodmanBuildOptions,
) -> Vec<String> {
    let mut args = PodmanArgs::from(["build", "-t", image, "-f", containerfile.as_str()]);

    for (name, value) in &options.build_args {
        args.key_value_option("--build-arg", name, value);
    }

    for (name, value) in &options.labels {
        args.key_value_option("--label", name, value);
    }

    args.flag(context_dir.as_str());
    args.into_vec()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use camino::Utf8Path;

    use super::*;
    use crate::podman::args::strings;

    #[test]
    fn build_image_args_are_stable_and_complete() {
        let options = PodmanBuildOptions {
            build_args: BTreeMap::from([
                ("AGENTBOX_RUNTIME".to_string(), "opencode".to_string()),
                ("OPENCODE_NPM_VERSION".to_string(), "1.2.3".to_string()),
            ]),
            labels: BTreeMap::from([
                (
                    "io.agentbox.opencode.package".to_string(),
                    "opencode-ai".to_string(),
                ),
                (
                    "io.agentbox.opencode.version".to_string(),
                    "1.2.3".to_string(),
                ),
            ]),
        };

        assert_eq!(
            build_image_args(
                "localhost/agentbox-opencode:ctx-0123456789abcdef",
                Utf8Path::new("/tmp/context/Containerfile"),
                Utf8Path::new("/tmp/context"),
                &options,
            ),
            strings([
                "build",
                "-t",
                "localhost/agentbox-opencode:ctx-0123456789abcdef",
                "-f",
                "/tmp/context/Containerfile",
                "--build-arg",
                "AGENTBOX_RUNTIME=opencode",
                "--build-arg",
                "OPENCODE_NPM_VERSION=1.2.3",
                "--label",
                "io.agentbox.opencode.package=opencode-ai",
                "--label",
                "io.agentbox.opencode.version=1.2.3",
                "/tmp/context",
            ])
        );
    }
}
