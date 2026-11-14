//! Printing and yes/no confirmation for commands that ask before deleting.

use console::Style;
use dialoguer::{Confirm, theme::ColorfulTheme};

use crate::cli::{
    errors::{CliErrors, CliResult},
    table,
};

/// Where a command prints and how it asks for confirmation.
///
/// Commands take this instead of writing to the terminal, so tests can record the
/// output and answer the question.
pub trait Prompt {
    fn print(&mut self, line: &str);
    /// Asks a yes/no question; the default answer is no.
    fn confirm(&mut self, question: &str) -> CliResult<bool>;
}

/// The real terminal.
#[derive(Debug)]
pub struct Terminal;

impl Prompt for Terminal {
    fn print(&mut self, line: &str) {
        println!("{line}");
    }

    fn confirm(&mut self, question: &str) -> CliResult<bool> {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(question)
            .default(false)
            .interact()
            .map_err(|error| CliErrors::ConfirmationFailed(error.to_string()))
    }
}

/// Asks `question` unless `yes` is set; prints a notice when the answer is no.
pub fn confirmed(
    yes: bool,
    question: &str,
    colors: bool,
    prompt: &mut impl Prompt,
) -> CliResult<bool> {
    prompt.print("");
    if yes || prompt.confirm(question)? {
        return Ok(true);
    }
    prompt.print(&table::paint(
        &Style::new().yellow(),
        "Aborted. Nothing was changed.",
        colors,
    ));
    Ok(false)
}

/// A [`Prompt`] for tests.
#[cfg(test)]
pub mod testing {
    use super::Prompt;
    use crate::cli::errors::{CliErrors, CliResult};

    /// Records what is printed and answers every question with a fixed reply.
    ///
    /// With no reply, a question fails the way it does without a terminal.
    #[derive(Debug)]
    pub struct Recorder {
        answer: Option<bool>,
        pub lines: Vec<String>,
        pub questions: Vec<String>,
    }

    impl Recorder {
        pub fn answering(answer: Option<bool>) -> Self {
            Self {
                answer,
                lines: Vec::new(),
                questions: Vec::new(),
            }
        }
    }

    impl Prompt for Recorder {
        fn print(&mut self, line: &str) {
            self.lines.push(line.to_owned());
        }

        fn confirm(&mut self, question: &str) -> CliResult<bool> {
            self.questions.push(question.to_owned());
            self.answer
                .ok_or_else(|| CliErrors::ConfirmationFailed("no terminal".to_owned()))
        }
    }
}
