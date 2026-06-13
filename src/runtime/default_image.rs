// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;
use std::sync::OnceLock;

use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::RuntimeKind;
use crate::digest;
use crate::{Error, Result};

const IMAGE_CONTEXT_HASH_LEN: usize = 16;
const IMAGE_CONTEXT_HASH_TAG_PREFIX: &str = "ctx-";

static DEFAULT_IMAGE_CONTEXT_HASH: OnceLock<String> = OnceLock::new();

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
        relative_path: "profile.d/agentbox-runtime.sh",
        contents: include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/image/profile.d/agentbox-runtime.sh"
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

pub fn embedded_default_image_paths() -> impl Iterator<Item = &'static str> {
    EMBEDDED_DEFAULT_IMAGE_FILES
        .iter()
        .map(|file| file.relative_path)
}

pub fn default_image_context_hash() -> &'static str {
    DEFAULT_IMAGE_CONTEXT_HASH.get_or_init(|| image_context_hash(EMBEDDED_DEFAULT_IMAGE_FILES))
}

pub fn default_image(runtime: RuntimeKind) -> String {
    format!(
        "localhost/agentbox-{}:{IMAGE_CONTEXT_HASH_TAG_PREFIX}{}",
        runtime.as_str(),
        default_image_context_hash()
    )
}

pub fn is_content_hash_default_image_ref(runtime: RuntimeKind, image: &str) -> bool {
    let prefix = format!(
        "localhost/agentbox-{}:{IMAGE_CONTEXT_HASH_TAG_PREFIX}",
        runtime.as_str()
    );
    image
        .strip_prefix(&prefix)
        .is_some_and(is_default_image_context_hash)
}

pub(crate) fn is_default_image_context_hash(value: &str) -> bool {
    value.len() == IMAGE_CONTEXT_HASH_LEN && value.chars().all(|ch| ch.is_ascii_hexdigit())
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

fn image_context_hash(files: &[EmbeddedDefaultImageFile]) -> String {
    let mut hasher = Sha256::new();

    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update([0]);
        hasher.update(file.contents.len().to_string().as_bytes());
        hasher.update([0]);
        hasher.update(file.contents);
        hasher.update([0]);
    }

    digest::hex_prefix(hasher.finalize(), IMAGE_CONTEXT_HASH_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &[EmbeddedDefaultImageFile] = &[
        EmbeddedDefaultImageFile {
            relative_path: "Containerfile",
            contents: b"FROM example\n",
        },
        EmbeddedDefaultImageFile {
            relative_path: "bootstrap",
            contents: b"#!/bin/sh\n",
        },
    ];

    #[test]
    fn image_context_hash_is_deterministic_over_paths_lengths_and_contents() {
        assert_eq!(image_context_hash(FILES), image_context_hash(FILES));
        assert_ne!(
            image_context_hash(FILES),
            image_context_hash(&[
                EmbeddedDefaultImageFile {
                    relative_path: "Containerfile",
                    contents: b"FROM example\n",
                },
                EmbeddedDefaultImageFile {
                    relative_path: "bootstrap",
                    contents: b"#!/bin/bash\n",
                },
            ])
        );
        assert_ne!(
            image_context_hash(FILES),
            image_context_hash(&[
                EmbeddedDefaultImageFile {
                    relative_path: "bootstrap",
                    contents: b"#!/bin/sh\n",
                },
                EmbeddedDefaultImageFile {
                    relative_path: "Containerfile",
                    contents: b"FROM example\n",
                },
            ])
        );
    }

    #[test]
    fn embedded_image_inputs_exclude_docs_dev_files_and_tests() {
        let paths = embedded_default_image_paths().collect::<Vec<_>>();

        assert_eq!(
            paths,
            [
                "Containerfile",
                "bootstrap",
                "entrypoint",
                "lib/runtime-contract.sh",
                "profile.d/agentbox-runtime.sh",
                "runtime-packages.nix",
            ]
        );
        assert!(!paths.contains(&"README.md"));
        assert!(!paths.contains(&"justfile"));
        assert!(!paths.iter().any(|path| path.starts_with("tests/")));
    }

    #[test]
    fn default_image_ref_uses_context_hash_tag() {
        let image = default_image(RuntimeKind::Opencode);
        let hash = default_image_context_hash();

        assert_eq!(hash.len(), IMAGE_CONTEXT_HASH_LEN);
        assert!(is_default_image_context_hash(hash));
        assert_eq!(image, format!("localhost/agentbox-opencode:ctx-{hash}"));
        assert!(is_content_hash_default_image_ref(
            RuntimeKind::Opencode,
            &image
        ));
        assert!(!is_content_hash_default_image_ref(
            RuntimeKind::Opencode,
            "localhost/agentbox-opencode:local"
        ));
    }

    #[test]
    fn default_image_context_hash_validation_matches_tag_format() {
        assert!(is_default_image_context_hash("0123456789abcdef"));
        assert!(is_default_image_context_hash("ABCDEF0123456789"));
        assert!(!is_default_image_context_hash("0123456789abcde"));
        assert!(!is_default_image_context_hash("0123456789abcdef0"));
        assert!(!is_default_image_context_hash("0123456789abcdeg"));
    }
}
