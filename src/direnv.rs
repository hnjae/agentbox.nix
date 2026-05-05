// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use camino::Utf8Path;

use crate::preflight::direnv_applies_to_target;

pub fn wrap_exec_if_envrc_applies(
    argv: Vec<String>,
    target_directory: &Utf8Path,
    git_root: &Utf8Path,
) -> Vec<String> {
    if !direnv_applies_to_target(target_directory, git_root) {
        return argv;
    }

    let mut wrapped = Vec::with_capacity(argv.len() + 3);
    wrapped.extend(["direnv".to_string(), "exec".to_string(), ".".to_string()]);
    wrapped.extend(argv);
    wrapped
}
