#![allow(dead_code)]

// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

pub struct FakeBinDir {
    dir: TempDir,
}

impl FakeBinDir {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn install_exact_response(
        &self,
        name: &str,
        expected_args: &[&str],
        stdout: &str,
    ) -> PathBuf {
        self.install_program(name, expected_args, stdout, "", 0)
    }

    pub fn install_exact_failure(
        &self,
        name: &str,
        expected_args: &[&str],
        stderr: &str,
        exit_code: i32,
    ) -> PathBuf {
        self.install_program(name, expected_args, "", stderr, exit_code)
    }

    fn install_program(
        &self,
        name: &str,
        expected_args: &[&str],
        stdout: &str,
        stderr: &str,
        exit_code: i32,
    ) -> PathBuf {
        let path = self.dir.path().join(name);
        let script = render_program(name, expected_args, stdout, stderr, exit_code);
        write_executable(&path, &script);
        path
    }
}

pub fn write_executable(path: impl AsRef<Path>, content: &str) {
    let path = path.as_ref();
    fs::write(path, content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}

pub fn path_with_prepend(prepend: &Path, original_path: &str) -> String {
    format!("{}:{original_path}", prepend.display())
}

pub fn read_log_lines(path: &Path) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(contents) => contents.lines().map(|line| line.to_string()).collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("failed to read log `{}`: {error}", path.display()),
    }
}

pub fn operation_names(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .map(|line| line.split_whitespace().next().unwrap())
        .collect()
}

pub fn fake_git_script() -> &'static str {
    include_str!("../fixtures/bin/git.sh")
}

fn render_program(
    name: &str,
    expected_args: &[&str],
    stdout: &str,
    stderr: &str,
    exit_code: i32,
) -> String {
    let mut script = String::from("#!/bin/sh\nset -eu\n");
    script.push_str(&format!(
        "if [ \"$#\" -ne {} ]; then\n",
        expected_args.len()
    ));
    script.push_str(&format!(
        "  printf '%s\\n' {} >&2\n  exit 97\nfi\n",
        shell_single_quote(&format!(
            "unexpected arg count for {name}: got $# expected {}",
            expected_args.len()
        ))
    ));

    for (index, arg) in expected_args.iter().enumerate() {
        let position = index + 1;
        script.push_str(&format!(
            "if [ \"${{{position}}}\" != {} ]; then\n",
            shell_single_quote(arg)
        ));
        script.push_str(&format!(
            "  printf '%s\\n' {} >&2\n  exit 98\nfi\n",
            shell_single_quote(&format!(
                "unexpected arg {position} for {name}: got '${{{position}}}' expected '{arg}'"
            ))
        ));
    }

    if !stdout.is_empty() {
        script.push_str("cat <<'EOF'\n");
        script.push_str(stdout);
        if !stdout.ends_with('\n') {
            script.push('\n');
        }
        script.push_str("EOF\n");
    }

    if !stderr.is_empty() {
        script.push_str("cat <<'EOF' >&2\n");
        script.push_str(stderr);
        if !stderr.ends_with('\n') {
            script.push('\n');
        }
        script.push_str("EOF\n");
    }

    script.push_str(&format!("exit {exit_code}\n"));
    script
}

fn shell_single_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}
