#![allow(clippy::multiple_crate_versions)]

// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

fn main() -> std::process::ExitCode {
    agentbox::diagnostic::init_tracing();
    agentbox::main()
}
