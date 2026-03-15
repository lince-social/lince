use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::logic::{CollectionEntry, ErrorToast, InputMode, SqlViewTable, TuiState};

pub(super) fn render_header(frame: &mut Frame, area: Rect, state: &TuiState) {
    let lines = if state.show_collection_browser {
        render_collection_browser_lines(state)
    } else {
        vec![render_active_header_line(state)]
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_active_header_line(state: &TuiState) -> Line<'static> {
    match state.active_collection() {
        Some(entry) => render_collection_line(entry, true, state.selected_view),
        None => Line::from("No active collection"),
    }
}

fn render_collection_browser_lines(state: &TuiState) -> Vec<Line<'static>> {
    if state.collections.is_empty() {
        return vec![Line::from("No collections")];
    }

    state
        .collections
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            render_collection_line(
                entry,
                index == state.active_collection_index,
                state.selected_view,
            )
        })
        .collect()
}

fn render_collection_line(
    entry: &CollectionEntry,
    is_active_collection: bool,
    selected_view: usize,
) -> Line<'static> {
    let mut spans = Vec::new();
    let collection_style = if is_active_collection {
        Style::default()
            .fg(Color::Black)
            .bg(Color::LightCyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    spans.push(Span::styled(
        format!(" {} {} ", entry.collection.id, entry.collection.name),
        collection_style,
    ));

    if entry.views.is_empty() {
        spans.push(Span::styled(
            " no sql views ",
            Style::default().fg(Color::DarkGray),
        ));
        return Line::from(spans);
    }

    for (index, view) in entry.views.iter().enumerate() {
        spans.push(Span::raw(" "));
        spans.push(render_view_span(
            view.quantity,
            &view.name,
            is_active_collection && index == selected_view,
        ));
    }

    Line::from(spans)
}

fn render_view_span(quantity: i32, name: &str, is_selected: bool) -> Span<'static> {
    let base_style = if quantity == 1 {
        Style::default().fg(Color::LightBlue)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let style = if is_selected {
        base_style
            .fg(Color::Black)
            .bg(Color::LightRed)
            .add_modifier(Modifier::BOLD)
    } else {
        base_style
    };
    Span::styled(format!(" {} {} ", quantity, name), style)
}

pub(super) fn render_input(frame: &mut Frame, area: Rect, state: &TuiState) {
    let mode_label = match state.input_mode {
        InputMode::Normal => "-- NORMAL --",
        InputMode::Insert => "-- INSERT --",
    };
    let paragraph = Paragraph::new(vec![
        Line::styled(mode_label, Style::default().fg(Color::DarkGray)),
        Line::from(state.command_input.clone()),
    ])
    .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);

    if state.input_mode == InputMode::Insert {
        let cursor_x = area
            .x
            .saturating_add(state.command_input.chars().count() as u16)
            .min(area.x + area.width.saturating_sub(1));
        let cursor_y = area.y.saturating_add(1);
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

pub(super) fn render_error_toasts(frame: &mut Frame, state: &TuiState, input_area: Rect) {
    if state.error_toasts.is_empty() {
        return;
    }

    let toast_width = frame.area().width.min(50);
    let x = frame
        .area()
        .x
        .saturating_add(frame.area().width.saturating_sub(toast_width + 1));
    let mut next_bottom = input_area.y.saturating_sub(1);

    for ErrorToast { message, .. } in state.error_toasts.iter().rev() {
        if next_bottom < frame.area().y + 3 {
            break;
        }

        let area = Rect {
            x,
            y: next_bottom.saturating_sub(2),
            width: toast_width,
            height: 3,
        };
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(message.clone())
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::LightRed)),
            area,
        );

        next_bottom = area.y.saturating_sub(1);
    }
}

pub(super) fn render_help(frame: &mut Frame) {
    let area = centered_rect(frame.area(), 90, 80);
    let help_text = render_help_lines(area.width.saturating_sub(2) as usize);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Keybindings"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[derive(Clone, Copy)]
struct HelpEntry<'a> {
    bindings: &'a [&'a str],
    description: &'a str,
}

fn render_help_lines(inner_width: usize) -> Vec<Line<'static>> {
    let entries = [
        HelpEntry {
            bindings: &["Ctrl+/", "Ctrl+?"],
            description: "Open or close this help",
        },
        HelpEntry {
            bindings: &["x"],
            description: "Execute the focused row id as an operation",
        },
        HelpEntry {
            bindings: &["Ctrl+W"],
            description: "Dismiss newest error toast",
        },
        HelpEntry {
            bindings: &["Ctrl+C", "Ctrl+Q"],
            description: "Quit",
        },
        HelpEntry {
            bindings: &["Tab"],
            description: "Open or close the collections browser",
        },
        HelpEntry {
            bindings: &["h", "l"],
            description: "Normal mode: move focused cell across columns",
        },
        HelpEntry {
            bindings: &["j", "k"],
            description: "Normal mode: move focused cell across rows",
        },
        HelpEntry {
            bindings: &["i"],
            description: "Enter insert mode for the operation bar",
        },
        HelpEntry {
            bindings: &["Esc"],
            description: "Leave insert mode or close help",
        },
        HelpEntry {
            bindings: &["Alt+H"],
            description: "Previous view",
        },
        HelpEntry {
            bindings: &["Alt+L"],
            description: "Next view",
        },
        HelpEntry {
            bindings: &["Alt+J"],
            description: "Next collection",
        },
        HelpEntry {
            bindings: &["Alt+K"],
            description: "Previous collection",
        },
        HelpEntry {
            bindings: &["Alt+Space"],
            description: "Toggle selected view between 0 and 1",
        },
        HelpEntry {
            bindings: &["Up", "Down"],
            description: "Scroll rows",
        },
        HelpEntry {
            bindings: &["PageUp", "PageDown"],
            description: "Page rows",
        },
        HelpEntry {
            bindings: &["Home", "End"],
            description: "Jump to top or bottom",
        },
        HelpEntry {
            bindings: &["Type"],
            description: "Insert mode: write into operation bar",
        },
        HelpEntry {
            bindings: &["Enter"],
            description: "Insert mode: execute operation",
        },
        HelpEntry {
            bindings: &["Backspace"],
            description: "Delete one character",
        },
        HelpEntry {
            bindings: &["Tab", "Alt+J", "Alt+K"],
            description: "Use Tab with Alt+J or Alt+K to cycle collections while the browser stays open",
        },
    ];

    let column_gap = 3usize;
    let binding_width = entries
        .iter()
        .flat_map(|entry| entry.bindings.iter())
        .map(|binding| binding.chars().count())
        .max()
        .unwrap_or(0)
        .min(inner_width.saturating_div(3).max(10));
    let description_width = inner_width
        .saturating_sub(binding_width + column_gap)
        .max(1);
    let binding_style = Style::default()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::BOLD);
    let separator_style = Style::default().fg(Color::DarkGray);
    let description_style = Style::default().fg(Color::White);

    let mut lines = vec![
        Line::styled(
            "Bindings",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
    ];

    for entry in entries {
        let binding_lines = wrap_bindings(entry.bindings, binding_width);
        let description_lines = wrap_text(entry.description, description_width);
        let row_height = binding_lines.len().max(description_lines.len());

        for index in 0..row_height {
            let binding = binding_lines.get(index).map(String::as_str).unwrap_or("");
            let description = description_lines
                .get(index)
                .map(String::as_str)
                .unwrap_or("");
            lines.push(Line::from(vec![
                Span::styled(pad_to_width(binding, binding_width), binding_style),
                Span::styled(" │ ", separator_style),
                Span::styled(description.to_owned(), description_style),
            ]));
        }

        lines.push(Line::from(""));
    }

    lines.pop();
    lines
}

fn wrap_bindings(bindings: &[&str], width: usize) -> Vec<String> {
    let mut lines = Vec::new();

    for binding in bindings {
        let wrapped = wrap_text(binding, width);
        if wrapped.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrapped);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let word_width = word.chars().count();
        if current.is_empty() {
            if word_width <= width {
                current.push_str(word);
                continue;
            }

            lines.extend(wrap_long_word(word, width));
            continue;
        }

        let candidate_width = current.chars().count() + 1 + word_width;
        if candidate_width <= width {
            current.push(' ');
            current.push_str(word);
            continue;
        }

        lines.push(current);
        current = String::new();

        if word_width <= width {
            current.push_str(word);
        } else {
            lines.extend(wrap_long_word(word, width));
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn wrap_long_word(word: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for ch in word.chars() {
        if current.chars().count() == width {
            lines.push(current);
            current = String::new();
        }
        current.push(ch);
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

fn pad_to_width(text: &str, width: usize) -> String {
    let padding = width.saturating_sub(text.chars().count());
    format!("{text}{}", " ".repeat(padding))
}

pub(super) fn render_sql_table_lines(
    table: &SqlViewTable,
    row_offset: usize,
    visible_rows: usize,
    inner_width: usize,
    selected_row: usize,
    selected_col: usize,
) -> Vec<Line<'static>> {
    let widths = table_widths(table, inner_width);
    let separator = "-".repeat(widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 3);
    let mut lines = vec![
        format_table_line(&table.headers, &widths, None, true),
        Line::from(separator),
    ];

    lines.extend(
        table
            .rows
            .iter()
            .enumerate()
            .skip(row_offset)
            .take(visible_rows)
            .map(|(row_index, row)| {
                format_table_line(
                    row,
                    &widths,
                    (row_index == selected_row).then_some(selected_col),
                    false,
                )
            }),
    );

    lines
}

fn table_widths(table: &SqlViewTable, inner_width: usize) -> Vec<usize> {
    let mut widths = table
        .headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            let mut width = header.chars().count();
            for row in &table.rows {
                width = width.max(
                    row.get(index)
                        .map(|value| value.chars().count())
                        .unwrap_or(0),
                );
            }
            width.min(32)
        })
        .collect::<Vec<_>>();

    fit_widths_to_area(&mut widths, inner_width);
    widths
}

fn fit_widths_to_area(widths: &mut [usize], inner_width: usize) {
    let separator_width = widths.len().saturating_sub(1) * 3;
    let min_width = if widths.len() <= 2 { 4 } else { 3 };
    let min_total = widths.len() * min_width + separator_width;

    if inner_width <= min_total {
        for width in widths.iter_mut() {
            *width = min_width;
        }
        return;
    }

    let mut total = widths.iter().sum::<usize>() + separator_width;
    while total > inner_width {
        if let Some((index, _)) = widths
            .iter()
            .enumerate()
            .filter(|(_, width)| **width > min_width)
            .max_by_key(|(_, width)| **width)
        {
            widths[index] -= 1;
            total -= 1;
        } else {
            break;
        }
    }
}

fn format_table_line(
    values: &[String],
    widths: &[usize],
    selected_col: Option<usize>,
    is_header: bool,
) -> Line<'static> {
    let mut spans = Vec::new();

    for (index, (value, width)) in values.iter().zip(widths.iter()).enumerate() {
        if index > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        }

        let mut style = if is_header {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        if selected_col == Some(index) {
            style = style.fg(Color::Black).bg(Color::Gray);
        }
        spans.push(Span::styled(pad_or_truncate(value, *width), style));
    }

    Line::from(spans)
}

fn pad_or_truncate(value: &str, width: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let len = chars.len();

    if len == width {
        return value.to_string();
    }

    if len < width {
        return format!("{value:<width$}");
    }

    if width == 0 {
        return String::new();
    }

    if width == 1 {
        return "…".to_string();
    }

    let mut truncated = chars.into_iter().take(width - 1).collect::<String>();
    truncated.push('…');
    truncated
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Percentage((100 - height_percent) / 2),
        ratatui::layout::Constraint::Percentage(height_percent),
        ratatui::layout::Constraint::Percentage((100 - height_percent) / 2),
    ])
    .split(area);
    ratatui::layout::Layout::horizontal([
        ratatui::layout::Constraint::Percentage((100 - width_percent) / 2),
        ratatui::layout::Constraint::Percentage(width_percent),
        ratatui::layout::Constraint::Percentage((100 - width_percent) / 2),
    ])
    .split(vertical[1])[1]
}
