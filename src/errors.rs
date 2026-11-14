//! Top-level error type for the `workbench` binary.

use crate::cli::errors::CliErrors;

/// The result type used by the command entry points.
pub type WorkbenchResult<T> = Result<T, WorkbenchErrors>;

/// Every error the binary can report to the user.
#[derive(Debug)]
pub enum WorkbenchErrors {
    /// An error from a CLI command, such as `init` or `project add`.
    CliErrors(CliErrors),
}

impl std::fmt::Display for WorkbenchErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkbenchErrors::CliErrors(cli_errors) => cli_errors.fmt(f),
        }
    }
}

impl std::error::Error for WorkbenchErrors {}

impl WorkbenchErrors {
    /// The message for the terminal, coloured when `colors` is set.
    pub fn render(&self, colors: bool) -> String {
        match self {
            WorkbenchErrors::CliErrors(cli_errors) => cli_errors.render(colors),
        }
    }
}

impl From<CliErrors> for WorkbenchErrors {
    fn from(value: CliErrors) -> Self {
        Self::CliErrors(value)
    }
}
