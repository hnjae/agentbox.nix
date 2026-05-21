#![allow(dead_code)]

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLog {
    entries: Vec<CommandLogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLogEntry {
    raw: String,
    operation: String,
    lock: String,
    args: String,
}

impl CommandLog {
    pub fn from_lines(lines: Vec<String>) -> Self {
        let entries = lines.into_iter().map(CommandLogEntry::parse).collect();
        Self { entries }
    }

    pub fn operation_names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(CommandLogEntry::operation)
            .collect()
    }

    pub fn first(&self, operation: &str) -> &CommandLogEntry {
        self.entries
            .iter()
            .find(|entry| entry.operation() == operation)
            .unwrap_or_else(|| panic!("expected `{operation}` invocation in command log"))
    }

    pub fn entry(&self, index: usize) -> &CommandLogEntry {
        &self.entries[index]
    }

    pub fn contains_operation(&self, operation: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.operation() == operation)
    }

    pub fn entries(&self) -> &[CommandLogEntry] {
        &self.entries
    }
}

impl CommandLogEntry {
    fn parse(raw: String) -> Self {
        let (operation, rest) = raw
            .split_once(' ')
            .unwrap_or_else(|| panic!("malformed command log entry `{raw}`"));
        let rest = rest
            .strip_prefix("lock=")
            .unwrap_or_else(|| panic!("missing lock field in command log entry `{raw}`"));
        let (lock, args) = rest
            .split_once(" args=")
            .unwrap_or_else(|| panic!("missing args field in command log entry `{raw}`"));

        let operation = operation.to_string();
        let lock = lock.to_string();
        let args = args.to_string();

        Self {
            raw,
            operation,
            lock,
            args,
        }
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn lock(&self) -> &str {
        &self.lock
    }

    pub fn args(&self) -> &str {
        &self.args
    }

    pub fn assert_lock_held(&self) {
        assert_eq!(
            self.lock, "held",
            "expected `{}` command lock to be held, got `{}` in `{}`",
            self.operation, self.lock, self.raw,
        );
    }

    pub fn assert_args_contain(&self, expected: &str) {
        assert!(
            self.args.contains(expected),
            "expected `{}` command args to contain `{expected}`; args were `{}`",
            self.operation,
            self.args,
        );
    }

    pub fn assert_args_do_not_contain(&self, unexpected: &str) {
        assert!(
            !self.args.contains(unexpected),
            "expected `{}` command args not to contain `{unexpected}`; args were `{}`",
            self.operation,
            self.args,
        );
    }

    pub fn assert_raw_contains(&self, expected: &str) {
        assert!(
            self.raw.contains(expected),
            "expected `{}` command log entry to contain `{expected}`; entry was `{}`",
            self.operation,
            self.raw,
        );
    }
}

pub fn operation_names(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .map(|line| line.split_whitespace().next().unwrap())
        .collect()
}
