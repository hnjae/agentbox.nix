// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;

use crate::preflight::{
    ETC_NIX_DESTINATION, ETC_STATIC_NIX_DESTINATION, NIX_CLIENT_DESTINATION, NIX_STORE_DESTINATION,
    PreflightReport,
};
use crate::runtime::{RuntimeAdapter, RuntimeCreateSpec, RuntimeExecSpec, RuntimeKind};
use crate::workspace::WorkspaceIdentity;
use crate::{Error, Result};

pub const RUNTIME_NAME: &str = "opencode";
pub const DEFAULT_IMAGE: &str = "localhost/agentbox-opencode:local";

const EMBEDDED_DEFAULT_IMAGE_FILES: &[EmbeddedDefaultImageFile] = &[
    EmbeddedDefaultImageFile {
        relative_path: "Containerfile",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/image/Containerfile"
        )),
    },
    EmbeddedDefaultImageFile {
        relative_path: "bootstrap",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/image/bootstrap"
        )),
    },
    EmbeddedDefaultImageFile {
        relative_path: "entrypoint",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/image/entrypoint"
        )),
    },
    EmbeddedDefaultImageFile {
        relative_path: "lib/runtime-contract.sh",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/image/lib/runtime-contract.sh"
        )),
    },
    EmbeddedDefaultImageFile {
        relative_path: "runtime-packages.nix",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/image/runtime-packages.nix"
        )),
    },
];

#[derive(Debug, Clone, Default)]
pub struct OpencodeRuntime;

#[derive(Debug)]
struct EmbeddedDefaultImageFile {
    relative_path: &'static str,
    contents: &'static [u8],
}

#[derive(Debug)]
pub struct DefaultImageBuildContext {
    _tempdir: TempDir,
    root: Utf8PathBuf,
}

impl DefaultImageBuildContext {
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    pub fn containerfile(&self) -> Utf8PathBuf {
        self.root.join("Containerfile")
    }
}

impl OpencodeRuntime {
    pub fn new() -> Self {
        Self
    }

    pub fn create_spec(
        &self,
        workspace: &WorkspaceIdentity,
        image: Option<&str>,
        preflight: &PreflightReport,
    ) -> RuntimeCreateSpec {
        RuntimeAdapter::new(RuntimeKind::Opencode).create_spec(
            workspace,
            image,
            &preflight.host_nix_mounts,
        )
    }

    pub fn foreground_command(&self) -> RuntimeExecSpec {
        RuntimeExecSpec {
            argv: RuntimeAdapter::new(RuntimeKind::Opencode)
                .server_command()
                .argv,
            detached: false,
        }
    }

    pub fn default_image_context(&self) -> Result<DefaultImageBuildContext> {
        materialize_default_image_context()
    }
}

pub fn required_host_mount_destinations() -> [&'static str; 4] {
    [
        NIX_STORE_DESTINATION,
        NIX_CLIENT_DESTINATION,
        ETC_NIX_DESTINATION,
        ETC_STATIC_NIX_DESTINATION,
    ]
}

pub fn embedded_default_image_paths() -> &'static [&'static str] {
    &[
        "Containerfile",
        "bootstrap",
        "entrypoint",
        "lib/runtime-contract.sh",
        "runtime-packages.nix",
    ]
}

pub fn materialize_default_image_context() -> Result<DefaultImageBuildContext> {
    let tempdir = tempfile::Builder::new()
        .prefix("agentbox-default-image-")
        .tempdir()?;
    let root = Utf8PathBuf::from_path_buf(tempdir.path().to_path_buf()).map_err(|path| {
        Error::msg(format!(
            "default runtime image build context path is not valid UTF-8: {}",
            path.display()
        ))
    })?;

    for file in EMBEDDED_DEFAULT_IMAGE_FILES {
        let path = root.join(file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent.as_std_path())?;
        }
        fs::write(path.as_std_path(), file.contents)?;
    }

    Ok(DefaultImageBuildContext {
        _tempdir: tempdir,
        root,
    })
}
