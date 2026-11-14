//! ASCII tables with optional terminal colours, shared by the commands that print them.

use console::Style;

/// Shown in place of a value that is not set.
pub const MISSING: &str = "-";

/// What a cell holds, which decides its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A project id.
    Id,
    /// A language, optionally followed by `/framework`.
    Language,
    DependencyManager,
    /// A field name in a two-column table.
    Label,
    /// Something about to be deleted, or a failure.
    Danger,
    /// Something that went well.
    Success,
    Plain,
}

/// The style of a body cell. Missing values are dimmed whatever their kind.
pub fn cell_style(kind: Kind, text: &str) -> Style {
    if text == MISSING {
        return Style::new().dim();
    }
    match kind {
        Kind::Id => Style::new().cyan().bold(),
        Kind::Language if text.starts_with("python") => Style::new().yellow(),
        Kind::Language if text.starts_with("typescript") => Style::new().blue(),
        Kind::DependencyManager => Style::new().magenta(),
        Kind::Label => Style::new().dim(),
        Kind::Danger => Style::new().red(),
        Kind::Success => Style::new().green(),
        Kind::Language | Kind::Plain => Style::new(),
    }
}

/// Applies `style` when `colors` is set, and returns the text unchanged otherwise.
///
/// Callers pass `colors` instead of it being read from the terminal, so output is exact in tests.
pub fn paint(style: &Style, text: &str, colors: bool) -> String {
    style
        .clone()
        .force_styling(colors)
        .apply_to(text)
        .to_string()
}

/// An indented bullet line, `  - text`, with the marker coloured.
///
/// `text` is expected to be painted already, so callers colour each part of it.
pub fn bullet(text: &str, colors: bool) -> String {
    format!(
        "  {} {text}",
        paint(&Style::new().cyan().bold(), "-", colors)
    )
}

/// Renders a table: a header row, then one row per entry, framed by `+---+` borders.
///
/// `kind` gives the kind of the cell in a column of a row, which decides its colour.
pub fn render<const COLUMNS: usize>(
    headers: [&str; COLUMNS],
    rows: &[[String; COLUMNS]],
    kind: impl Fn(usize, &[String; COLUMNS]) -> Kind,
    colors: bool,
) -> Vec<String> {
    let mut widths = headers.map(str::len);
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.len());
        }
    }

    let border = paint(&Style::new().dim(), &border_line(&widths), colors);
    let header = line(
        &widths,
        &headers.map(str::to_owned),
        |_, _| Style::new().bold(),
        colors,
    );

    let mut lines = vec![border.clone(), header, border.clone()];
    lines.extend(rows.iter().map(|row| {
        line(
            &widths,
            row,
            |column, text| cell_style(kind(column, row), text),
            colors,
        )
    }));
    lines.push(border);
    lines
}

/// `+------+----+`
fn border_line(widths: &[usize]) -> String {
    let segments: Vec<String> = widths.iter().map(|width| "-".repeat(width + 2)).collect();
    format!("+{}+", segments.join("+"))
}

/// `| cell | cell |`, padded on the plain text so colour codes do not skew the columns.
fn line<const COLUMNS: usize>(
    widths: &[usize; COLUMNS],
    cells: &[String; COLUMNS],
    style: impl Fn(usize, &str) -> Style,
    colors: bool,
) -> String {
    let separator = paint(&Style::new().dim(), "|", colors);
    let mut line = separator.clone();
    for (column, (cell, width)) in cells.iter().zip(widths).enumerate() {
        line.push_str(&paint(
            &style(column, cell),
            &format!(" {cell:<width$} "),
            colors,
        ));
        line.push_str(&separator);
    }
    line
}
