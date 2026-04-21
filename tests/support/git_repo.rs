// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use tempfile::TempDir;

pub fn temp_git_repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();

    fs::create_dir_all(repo.path().join(".git/refs/heads")).unwrap();
    fs::write(repo.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(
        repo.path().join(".git/config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tfilemode = true\n\tlogallrefupdates = true\n",
    )
    .unwrap();
    fs::write(repo.path().join(".gitignore"), "\n").unwrap();

    repo
}
