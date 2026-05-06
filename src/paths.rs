// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};

use crate::{Error, Result};

pub(crate) fn absolute_utf8_path(path: &Path) -> Result<Utf8PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    path_buf_to_utf8(absolute)
}

pub(crate) fn canonicalize_utf8_path(path: &Utf8Path) -> Result<Utf8PathBuf> {
    path_buf_to_utf8(std::fs::canonicalize(path.as_std_path())?)
}

pub(crate) fn path_buf_to_utf8(path: PathBuf) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(path).map_err(non_utf8_path_error)
}

pub(crate) fn path_is_or_descendant(path: &Utf8Path, root: &Utf8Path) -> bool {
    path == root || path.starts_with(root)
}

fn non_utf8_path_error(path: PathBuf) -> Error {
    Error::msg(format!("non-utf8 path: {path:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_or_descendant_matches_only_component_boundaries() {
        assert!(path_is_or_descendant(
            Utf8Path::new("/workspace/project"),
            Utf8Path::new("/workspace/project"),
        ));
        assert!(path_is_or_descendant(
            Utf8Path::new("/workspace/project/nested"),
            Utf8Path::new("/workspace/project"),
        ));
        assert!(!path_is_or_descendant(
            Utf8Path::new("/workspace/project-neighbor"),
            Utf8Path::new("/workspace/project"),
        ));
    }
}
