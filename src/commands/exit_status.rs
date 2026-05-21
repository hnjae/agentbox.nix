// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::process::ExitStatus;

use crate::{Error, Result};

use super::container_cleanup::{combine_error_with_cleanup_result, combined_error_message};

#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandFailureExit {
    Code(u8),
    Signal,
}

#[derive(Debug)]
pub(super) struct CommandExitFailure {
    exit: CommandFailureExit,
    error: Error,
}

impl CommandFailureExit {
    fn from_status(status: ExitStatus) -> Option<Self> {
        if status.success() {
            None
        } else if let Some(code) = status.code().and_then(exit_code) {
            Some(Self::Code(code))
        } else {
            Some(Self::Signal)
        }
    }
}

impl CommandExitFailure {
    pub(super) fn from_status(
        status: ExitStatus,
        error: impl FnOnce(ExitStatus) -> Error,
    ) -> Option<Self> {
        let exit = CommandFailureExit::from_status(status)?;

        Some(Self {
            exit,
            error: error(status),
        })
    }

    pub(super) fn into_direct_error(self) -> Error {
        match self.exit {
            CommandFailureExit::Code(code) => Error::ExitCode(code),
            CommandFailureExit::Signal => self.error,
        }
    }

    pub(super) fn into_error_with_cleanup_result(self, cleanup: Result<()>) -> Error {
        match self.exit {
            CommandFailureExit::Code(code) => match cleanup {
                Ok(()) => Error::ExitCode(code),
                Err(cleanup_error) => Error::ExitCodeWithMessage {
                    code,
                    message: combined_error_message(&self.error, &cleanup_error),
                },
            },
            CommandFailureExit::Signal => combine_error_with_cleanup_result(self.error, cleanup),
        }
    }
}

pub(super) fn exit_code(code: i32) -> Option<u8> {
    u8::try_from(code).ok()
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    #[test]
    fn success_status_has_no_failure() {
        assert!(
            CommandExitFailure::from_status(exit_status(0), |_| Error::msg("failed")).is_none()
        );
    }

    #[test]
    fn direct_numeric_exit_propagates_process_code_without_error_text() {
        let failure =
            CommandExitFailure::from_status(exit_status(42), |_| Error::msg("child failed"))
                .unwrap();

        assert_eq!(
            failure.into_direct_error().to_string(),
            "process exited with code 42"
        );
    }

    #[test]
    fn direct_signal_exit_uses_contextual_error() {
        let failure =
            CommandExitFailure::from_status(signal_status(9), |_| Error::msg("child signaled"))
                .unwrap();

        assert_eq!(failure.into_direct_error().to_string(), "child signaled");
    }

    #[test]
    fn numeric_exit_with_successful_cleanup_preserves_exit_code_only() {
        let failure =
            CommandExitFailure::from_status(exit_status(42), |_| Error::msg("child failed"))
                .unwrap();

        assert_eq!(
            failure.into_error_with_cleanup_result(Ok(())).to_string(),
            "process exited with code 42"
        );
    }

    #[test]
    fn numeric_exit_with_failed_cleanup_keeps_code_and_reports_both_errors() {
        let failure =
            CommandExitFailure::from_status(exit_status(42), |_| Error::msg("child failed"))
                .unwrap();

        let error = failure.into_error_with_cleanup_result(Err(Error::msg("cleanup failed")));

        assert_eq!(
            error.to_string(),
            "child failed; additionally, cleanup failed"
        );
        assert!(matches!(error, Error::ExitCodeWithMessage { code: 42, .. }));
    }

    #[test]
    fn signal_exit_with_failed_cleanup_reports_both_errors() {
        let failure =
            CommandExitFailure::from_status(signal_status(9), |_| Error::msg("child signaled"))
                .unwrap();

        assert_eq!(
            failure
                .into_error_with_cleanup_result(Err(Error::msg("cleanup failed")))
                .to_string(),
            "child signaled; additionally, cleanup failed"
        );
    }

    fn exit_status(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    fn signal_status(signal: i32) -> ExitStatus {
        ExitStatus::from_raw(signal)
    }
}
