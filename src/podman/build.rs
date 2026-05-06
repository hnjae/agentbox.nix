// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::Utf8Path;

use super::PodmanBuildOptions;
use super::args::PodmanArgs;

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
