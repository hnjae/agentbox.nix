// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::{self, IsTerminal};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::diagnostic;

const STEADY_TICK_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(crate) struct ProgressTask {
    progress: Option<ProgressBar>,
}

impl ProgressTask {
    pub(crate) fn start(message: impl Into<String>, verbose: bool) -> Self {
        let message = message.into();
        if verbose || !stderr_supports_progress() {
            diagnostic::info(message);
            return Self { progress: None };
        }

        let progress = ProgressBar::new_spinner();
        progress.set_draw_target(ProgressDrawTarget::stderr());
        progress.set_style(progress_style());
        progress.set_message(message);
        progress.enable_steady_tick(STEADY_TICK_INTERVAL);
        Self {
            progress: Some(progress),
        }
    }

    pub(crate) fn set_stage(&self, message: impl Into<String>) {
        let message = message.into();
        match &self.progress {
            Some(progress) => progress.set_message(message),
            None => diagnostic::info(message),
        }
    }

    pub(crate) fn clear(&mut self) {
        if let Some(progress) = self.progress.take() {
            progress.finish_and_clear();
        }
    }
}

impl Drop for ProgressTask {
    fn drop(&mut self) {
        self.clear();
    }
}

fn stderr_supports_progress() -> bool {
    io::stderr().is_terminal() && std::env::var_os("TERM").is_none_or(|term| term != "dumb")
}

fn progress_style() -> ProgressStyle {
    let template = if std::env::var_os("NO_COLOR").is_some() {
        "{spinner} [{elapsed_precise}] {msg}"
    } else {
        "{spinner:.cyan} [{elapsed_precise}] {msg}"
    };
    ProgressStyle::with_template(template).expect("static progress template must be valid")
}
