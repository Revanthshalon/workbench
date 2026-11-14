//! Detailed error logs in `.workbench/logs/`, one file per day.
//!
//! The terminal only shows a short, friendly message; the details that help track a
//! problem down (the full error, the file, the operating-system error) go here.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::cli::constants::{
    LOG_FILE_EXTENSION, LOG_FILE_PREFIX, WORKBENCH_DIRECTORY, WORKBENCH_LOGS_DIRECTORY,
};

const SECONDS_PER_DAY: u64 = 86_400;

/// Appends error entries to today's log file in a workspace.
#[derive(Debug)]
pub struct Logger {
    path: PathBuf,
    command: &'static str,
}

impl Logger {
    /// A logger for `command`, writing to `<root>/.workbench/logs/workbench-YYYY-MM-DD.log`.
    pub fn new(workspace_root: &Path, command: &'static str) -> Self {
        let (date, _) = utc_now();
        Self {
            path: workspace_root
                .join(WORKBENCH_DIRECTORY)
                .join(WORKBENCH_LOGS_DIRECTORY)
                .join(format!("{LOG_FILE_PREFIX}{date}.{LOG_FILE_EXTENSION}")),
            command,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends an error entry. Logging is best effort: a log that cannot be written
    /// never fails the command, so this only reports whether it worked.
    pub fn error(&self, message: &str) -> bool {
        self.write("ERROR", message).is_ok()
    }

    fn write(&self, level: &str, message: &str) -> std::io::Result<()> {
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory)?;
        }
        let (_, timestamp) = utc_now();
        // Continuation lines are indented so every entry starts with its timestamp.
        let message = message.trim_end().replace('\n', "\n    ");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{timestamp} {level} [{}] {message}", self.command)
    }
}

/// Today's date (`2026-11-14`) and the current time (`2026-11-14T09:30:05Z`), in UTC.
fn utc_now() -> (String, String) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let days = seconds / SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    let of_day = seconds % SECONDS_PER_DAY;
    let date = format!("{year:04}-{month:02}-{day:02}");
    let timestamp = format!(
        "{date}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    );
    (date, timestamp)
}

/// The calendar date of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use super::{Logger, civil_from_days};

    #[test]
    fn converts_day_counts_to_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(20_771), (2026, 11, 14));
    }

    #[test]
    fn appends_timestamped_entries_to_a_daily_file() -> std::io::Result<()> {
        let root = env::temp_dir().join(format!("workbench-log-{}", process::id()));
        let logger = Logger::new(&root, "index");

        assert!(logger.error("first"));
        assert!(logger.error("second\ndetail"));

        let name = logger
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let contents = fs::read_to_string(logger.path());
        fs::remove_dir_all(&root)?;

        assert!(name.starts_with("workbench-") && name.ends_with(".log"));
        let contents = contents?;
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].ends_with("Z ERROR [index] first"));
        assert!(lines[1].ends_with("Z ERROR [index] second"));
        assert_eq!(lines[2], "    detail");
        Ok(())
    }
}
