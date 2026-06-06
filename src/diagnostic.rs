// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::io::{self, IsTerminal, Write};

use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};
use tracing::field::{Field, Visit};
use tracing::{Event, Level};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{Layer, Registry};

const TIMESTAMP_FORMAT: &[FormatItem<'_>] = format_description!(
    "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"
);
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BRIGHT_BLACK: &str = "\x1b[90m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_BLUE: &str = "\x1b[34m";
const ANSI_BOLD_BRIGHT_CYAN: &str = "\x1b[1;96m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Debug,
}

impl Severity {
    fn label(self) -> &'static str {
        match self {
            Self::Error => "ERR",
            Self::Warning => "WARNING",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }

    fn ansi(self) -> &'static str {
        match self {
            Self::Error => ANSI_RED,
            Self::Warning => ANSI_YELLOW,
            Self::Info => ANSI_BLUE,
            Self::Debug => ANSI_BRIGHT_BLACK,
        }
    }

    fn from_tracing_level(level: &Level) -> Self {
        match *level {
            Level::ERROR => Self::Error,
            Level::WARN => Self::Warning,
            Level::INFO => Self::Info,
            Level::DEBUG | Level::TRACE => Self::Debug,
        }
    }
}

pub fn init_tracing() {
    let subscriber = Registry::default().with(DiagnosticLayer.with_filter(LevelFilter::INFO));
    let _ = tracing::subscriber::set_global_default(subscriber);
}

pub fn error(message: impl AsRef<str>) {
    emit(Severity::Error, message);
}

pub fn warning(message: impl AsRef<str>) {
    emit(Severity::Warning, message);
}

pub fn info(message: impl AsRef<str>) {
    emit(Severity::Info, message);
}

pub(crate) fn info_rendered(render_message: impl FnOnce(bool) -> String) {
    emit_rendered(Severity::Info, render_message);
}

pub fn debug(message: impl AsRef<str>) {
    emit(Severity::Debug, message);
}

pub(crate) fn bold_bright_cyan(text: &str, color: bool) -> String {
    if color {
        format!("{ANSI_BOLD_BRIGHT_CYAN}{text}{ANSI_RESET}")
    } else {
        text.to_string()
    }
}

pub fn emit(severity: Severity, message: impl AsRef<str>) {
    emit_with(
        &mut io::stderr().lock(),
        timestamp_now(),
        severity,
        message.as_ref(),
        stderr_supports_color(),
    );
}

fn emit_rendered(severity: Severity, render_message: impl FnOnce(bool) -> String) {
    let color = stderr_supports_color();
    let message = render_message(color);
    emit_with(
        &mut io::stderr().lock(),
        timestamp_now(),
        severity,
        &message,
        color,
    );
}

fn emit_with(
    writer: &mut impl Write,
    timestamp: OffsetDateTime,
    severity: Severity,
    message: &str,
    color: bool,
) {
    if message.is_empty() {
        let _ = writeln!(writer, "{}", render_line(timestamp, severity, "", color));
        return;
    }

    for line in message.lines() {
        let _ = writeln!(writer, "{}", render_line(timestamp, severity, line, color));
    }
}

fn timestamp_now() -> OffsetDateTime {
    let now = OffsetDateTime::now_utc();
    match UtcOffset::current_local_offset() {
        Ok(offset) => now.to_offset(offset),
        Err(_) => now,
    }
}

fn stderr_supports_color() -> bool {
    io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn render_line(
    timestamp: OffsetDateTime,
    severity: Severity,
    message: &str,
    color: bool,
) -> String {
    let timestamp = timestamp
        .format(TIMESTAMP_FORMAT)
        .unwrap_or_else(|_| "0000-00-00T00:00:00+00:00".to_string());

    if color {
        format!(
            "{ANSI_BRIGHT_BLACK}[{timestamp}]{ANSI_RESET} {}{}{ANSI_RESET}: {message}",
            severity.ansi(),
            severity.label(),
        )
    } else {
        format!("[{timestamp}] {}: {message}", severity.label())
    }
}

#[derive(Debug)]
struct DiagnosticLayer;

impl<S> Layer<S> for DiagnosticLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventMessageVisitor::default();
        event.record(&mut visitor);
        let severity = Severity::from_tracing_level(event.metadata().level());
        emit(severity, visitor.finish());
    }
}

#[derive(Debug, Default)]
struct EventMessageVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl EventMessageVisitor {
    fn finish(self) -> String {
        match (self.message, self.fields.is_empty()) {
            (Some(message), true) => message,
            (Some(message), false) => format!("{} {}", message, self.fields.join(" ")),
            (None, true) => String::new(),
            (None, false) => self.fields.join(" "),
        }
    }

    fn record_value(&mut self, field: &Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }
}

impl Visit for EventMessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{value:?}"));
    }
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

    use super::*;

    fn sample_timestamp() -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::May, 6).unwrap(),
            Time::from_hms(22, 15, 56).unwrap(),
        )
        .assume_offset(UtcOffset::from_hms(9, 0, 0).unwrap())
    }

    #[test]
    fn renders_timestamp_severity_and_message_without_color() {
        assert_eq!(
            render_line(sample_timestamp(), Severity::Info, "message", false),
            "[2026-05-06T22:15:56+09:00] INFO: message"
        );
    }

    #[test]
    fn renders_ansi_color_when_enabled() {
        assert_eq!(
            render_line(sample_timestamp(), Severity::Error, "failed", true),
            "\x1b[90m[2026-05-06T22:15:56+09:00]\x1b[0m \x1b[31mERR\x1b[0m: failed"
        );
    }

    #[test]
    fn bold_bright_cyan_styles_selected_identifiers_when_color_enabled() {
        assert_eq!(bold_bright_cyan("devenv", true), "\x1b[1;96mdevenv\x1b[0m");
        assert_eq!(bold_bright_cyan("devenv", false), "devenv");
    }

    #[test]
    fn emits_each_input_line_as_a_log_line() {
        let mut output = Vec::new();

        emit_with(
            &mut output,
            sample_timestamp(),
            Severity::Debug,
            "first\nsecond\n",
            false,
        );

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "[2026-05-06T22:15:56+09:00] DEBUG: first\n[2026-05-06T22:15:56+09:00] DEBUG: second\n"
        );
    }
}
