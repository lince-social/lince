use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, HighlightSpacing, Paragraph, Row, Table, TableState, Wrap,
    },
};

use super::logic::{
    CollectionEntry, CreationModalState, ErrorToast, InputMode, SqlViewTable, TuiState,
};

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
    Span::styled(format!(" {} ", name), style)
}

pub(super) fn render_input(frame: &mut Frame, area: Rect, state: &TuiState) {
    let input_background = Color::Rgb(10, 10, 10);
    let mode_label = match state.input_mode {
        InputMode::Normal => "NORMAL",
        InputMode::Operation => "OPERATION",
        InputMode::CellInsert => "INSERT",
        InputMode::Visual => "VISUAL",
    };
    let mode_width = mode_label.len() as u16 + 2;
    let areas =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(mode_width)]).split(area);
    let input_area = areas[0];
    let mode_area = areas[1];

    frame.render_widget(
        Paragraph::new(Line::from("")).style(Style::default().bg(input_background)),
        area,
    );

    frame.render_widget(
        Paragraph::new(Line::styled(
            format!(" {} ", mode_label),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        mode_area,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            state.footer_input_text(),
            Style::default().fg(Color::White).bg(input_background),
        ))
        .wrap(Wrap { trim: false }),
        input_area,
    );

    if state.show_footer_cursor() {
        let cursor_x = input_area
            .x
            .saturating_add(state.footer_cursor_offset() as u16)
            .min(input_area.x + input_area.width.saturating_sub(1));
        let cursor_y = input_area.y;
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

pub(super) fn render_create_shortcut_picker(frame: &mut Frame, state: &TuiState, input_area: Rect) {
    if state.create_shortcut_filter().is_none() {
        return;
    }

    let options = state.create_shortcut_options();
    let filter = state.create_shortcut_filter().unwrap_or_default();
    let available_height = input_area
        .y
        .saturating_sub(frame.area().y.saturating_add(1)) as usize;
    if available_height < 4 {
        return;
    }
    let max_visible_option_count = available_height.saturating_sub(3).max(1);
    let visible_option_count = options.len().clamp(1, max_visible_option_count);
    let content_width = options
        .iter()
        .map(|(id, _, label)| format!("{id:>2}  {label}").chars().count())
        .max()
        .unwrap_or(18)
        .max("id filter: ".chars().count() + filter.chars().count())
        .max(18);
    let width = (content_width + 2).min(frame.area().width.saturating_sub(2) as usize) as u16 + 2;
    let height = (visible_option_count + 3) as u16;
    let x = frame
        .area()
        .x
        .saturating_add(frame.area().width.saturating_sub(width + 1));
    let y = input_area
        .y
        .saturating_sub(height)
        .max(frame.area().y.saturating_add(1));
    let area = Rect {
        x,
        y,
        width,
        height,
    };
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });

    let selected = state
        .create_shortcut_selection
        .min(options.len().saturating_sub(1));
    let start = if selected >= visible_option_count {
        selected + 1 - visible_option_count
    } else {
        0
    };

    let mut lines = Vec::new();
    let filter_text = if filter.is_empty() {
        "id filter: all".to_string()
    } else {
        format!("id filter: {filter}")
    };
    lines.push(Line::styled(
        filter_text,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));

    if options.is_empty() {
        lines.push(Line::styled(
            "No matching tables",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for (offset, (id, _, label)) in options
            .iter()
            .skip(start)
            .take(visible_option_count)
            .enumerate()
        {
            let is_selected = start + offset == selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::LightCyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::styled(format!("{id:>2}  {label}"), style));
        }
    }

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title("Create")
            .border_style(Style::default().fg(Color::LightCyan)),
        area,
    );
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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

pub(super) fn render_creation_modal(frame: &mut Frame, modal: &CreationModalState) {
    let area = centered_rect(frame.area(), 72, 82);
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    let inner_width = inner.width.saturating_sub(2) as usize;
    let active_value_style = Style::default()
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD);
    let inactive_value_style = Style::default().fg(Color::White);
    let active_label_style = Style::default()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::BOLD);
    let inactive_label_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::styled(
            "Tab/Shift+Tab move field | Enter newline | Ctrl+W submit | Esc close",
            Style::default().fg(Color::Gray),
        ),
        Line::from(""),
    ];
    let mut active_line_start = lines.len();

    if modal.fields.is_empty() {
        lines.push(Line::styled(
            "No editable columns. Press Ctrl+W to insert default values.",
            Style::default().fg(Color::White),
        ));
    } else {
        for (index, field) in modal.fields.iter().enumerate() {
            if index == modal.active_field {
                active_line_start = lines.len();
            }
            lines.push(Line::styled(
                field.name.clone(),
                if index == modal.active_field {
                    active_label_style
                } else {
                    inactive_label_style
                },
            ));

            let display_value = modal.field_value_with_cursor(index);
            let wrapped_lines = wrap_text_preserving_breaks(&display_value, inner_width.max(1));
            for wrapped_line in wrapped_lines {
                lines.push(Line::styled(
                    format!(" {wrapped_line}"),
                    if index == modal.active_field {
                        active_value_style
                    } else {
                        inactive_value_style
                    },
                ));
            }
            lines.push(Line::from(""));
        }
    }

    let visible_height = inner.height as usize;
    let start = if active_line_start + 3 > visible_height {
        active_line_start.saturating_sub(2)
    } else {
        0
    };
    let visible_lines = lines
        .into_iter()
        .skip(start)
        .take(visible_height.max(1))
        .collect::<Vec<_>>();

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Create {}", modal.table_name())),
        area,
    );
    frame.render_widget(
        Paragraph::new(visible_lines).wrap(Wrap { trim: false }),
        inner,
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
            bindings: &["Ctrl+T", "Ctrl+T 4"],
            description: "Open the create-table picker; type table ids to filter it",
        },
        HelpEntry {
            bindings: &["Ctrl+W"],
            description: "Save the focused cell if edited, otherwise dismiss the newest error toast",
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
            description: "Enter insert mode in the focused cell",
        },
        HelpEntry {
            bindings: &["o"],
            description: "Focus the operation input bar",
        },
        HelpEntry {
            bindings: &["v"],
            description: "Start a visual text selection in the focused cell",
        },
        HelpEntry {
            bindings: &["Esc"],
            description: "Return to normal mode without saving, or close help",
        },
        HelpEntry {
            bindings: &["Ctrl+U", "Ctrl+D"],
            description: "Normal mode: move the focused row half a page",
        },
        HelpEntry {
            bindings: &["Up", "Down", "j", "k", "Tab", "Shift+Tab"],
            description: "Create picker: move through the table options",
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
            description: "Insert mode: write into the focused cell",
        },
        HelpEntry {
            bindings: &["Enter"],
            description: "Operation mode: execute the current operation",
        },
        HelpEntry {
            bindings: &["Ctrl+W"],
            description: "Creation modal: submit the current row",
        },
        HelpEntry {
            bindings: &["Backspace"],
            description: "Delete one character in operation or cell insert mode",
        },
        HelpEntry {
            bindings: &["y"],
            description: "Visual mode: copy the selection to the system clipboard",
        },
        HelpEntry {
            bindings: &["d"],
            description: "Visual mode: delete the selected text",
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

fn wrap_text_preserving_breaks(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in text.chars() {
        if ch == '\n' {
            lines.push(current);
            current = String::new();
            current_width = 0;
            continue;
        }

        if current_width == width {
            lines.push(current);
            current = String::new();
            current_width = 0;
        }

        current.push(ch);
        current_width += 1;
    }

    if !current.is_empty() || lines.is_empty() || text.ends_with('\n') {
        lines.push(current);
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

fn pad_center_to_width(text: &str, width: usize) -> String {
    let text_width = text.chars().count();
    if text_width >= width {
        return text.chars().take(width).collect();
    }

    let total_padding = width - text_width;
    let left_padding = total_padding / 2;
    let right_padding = total_padding - left_padding;
    format!(
        "{}{}{}",
        " ".repeat(left_padding),
        text,
        " ".repeat(right_padding)
    )
}

pub(super) fn render_sql_table(
    frame: &mut Frame,
    area: Rect,
    table: &SqlViewTable,
    state: &TuiState,
    inner_width: usize,
    row_offset: usize,
) {
    let widths = table_widths(table, inner_width);
    let non_wrapping = table
        .headers
        .iter()
        .map(|header| is_non_wrapping_column(header))
        .collect::<Vec<_>>();
    let header = Row::new(table.headers.iter().enumerate().map(|(index, value)| {
        styled_table_cell(
            value,
            widths[index],
            index == 0,
            true,
            index == widths.len().saturating_sub(1),
            true,
            non_wrapping[index],
            true,
            2,
            false,
        )
    }))
    .height(3)
    .style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    let rows = table.rows.iter().enumerate().map(|(row_index, row)| {
        wrapped_table_row(
            row,
            state,
            &widths,
            &non_wrapping,
            row_index == table.rows.len().saturating_sub(1),
            row_index == state.selected_row,
        )
    });
    let constraints = widths
        .iter()
        .map(|width| Constraint::Length(*width as u16))
        .collect::<Vec<_>>();
    let widget = Table::new(rows, constraints)
        .header(header)
        .column_spacing(0)
        .row_highlight_style(Style::default())
        .column_highlight_style(Style::default())
        .cell_highlight_style(Style::default())
        .highlight_spacing(HighlightSpacing::Never);
    let mut table_state = TableState::new().with_offset(row_offset);

    frame.render_stateful_widget(widget, area, &mut table_state);
}

pub(super) fn sql_table_row_heights(
    table: &SqlViewTable,
    state: &TuiState,
    inner_width: usize,
) -> Vec<usize> {
    let widths = table_widths(table, inner_width);
    let non_wrapping = table
        .headers
        .iter()
        .map(|header| is_non_wrapping_column(header))
        .collect::<Vec<_>>();

    table
        .rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let wrapped_cells = row
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    if row_index == state.selected_row && index == state.selected_col {
                        wrapped_editor_cell_lines(
                            value,
                            widths[index],
                            true,
                            index == widths.len().saturating_sub(1),
                            non_wrapping[index],
                        )
                    } else {
                        wrapped_cell_lines(
                            value,
                            widths[index],
                            true,
                            index == widths.len().saturating_sub(1),
                            non_wrapping[index],
                        )
                    }
                })
                .collect::<Vec<_>>();
            wrapped_cells.iter().map(Vec::len).max().unwrap_or(1).max(1) + 1
        })
        .collect()
}

fn wrapped_table_row(
    values: &[String],
    state: &TuiState,
    widths: &[usize],
    non_wrapping: &[bool],
    is_last_row: bool,
    is_selected_row: bool,
) -> Row<'static> {
    let wrapped_cells = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let is_selected_cell = is_selected_row && index == state.selected_col;
            if is_selected_cell {
                wrapped_editor_cell_lines(
                    value,
                    widths[index],
                    true,
                    index == widths.len().saturating_sub(1),
                    non_wrapping[index],
                )
            } else {
                wrapped_cell_lines(
                    value,
                    widths[index],
                    true,
                    index == widths.len().saturating_sub(1),
                    non_wrapping[index],
                )
            }
        })
        .collect::<Vec<_>>();
    let height = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let cells = wrapped_cells
        .into_iter()
        .enumerate()
        .map(|(index, lines)| {
            if is_selected_row && index == state.selected_col {
                editor_cell_from_value(
                    &values[index],
                    widths[index],
                    index == 0,
                    true,
                    index == widths.len().saturating_sub(1),
                    non_wrapping[index],
                    height,
                    is_last_row,
                    state,
                )
            } else {
                cell_from_lines(
                    lines,
                    widths[index],
                    index == 0,
                    true,
                    index == widths.len().saturating_sub(1),
                    false,
                    false,
                    height,
                    true,
                    is_last_row,
                    false,
                )
            }
        })
        .collect::<Vec<_>>();

    Row::new(cells).height((height + 1) as u16)
}

fn styled_table_cell(
    value: &str,
    width: usize,
    is_first_column: bool,
    show_left_separator: bool,
    show_right_separator: bool,
    is_header: bool,
    non_wrapping: bool,
    draw_bottom_border: bool,
    row_height: usize,
    is_selected: bool,
) -> Cell<'static> {
    let lines = wrapped_cell_lines(
        value,
        width,
        show_left_separator,
        show_right_separator,
        non_wrapping,
    );
    cell_from_lines(
        lines,
        width,
        is_first_column,
        show_left_separator,
        show_right_separator,
        is_header,
        is_header,
        row_height,
        draw_bottom_border,
        false,
        is_selected,
    )
}

fn cell_from_lines(
    lines: Vec<String>,
    width: usize,
    is_first_column: bool,
    show_left_separator: bool,
    show_right_separator: bool,
    is_header: bool,
    draw_top_border: bool,
    row_height: usize,
    draw_bottom_border: bool,
    draw_final_bottom_border: bool,
    is_selected: bool,
) -> Cell<'static> {
    let content_width =
        width.saturating_sub(show_left_separator as usize + show_right_separator as usize);
    let style = if is_header {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if is_selected {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };
    let content_lines = lines
        .into_iter()
        .map(|line| {
            let rendered = if is_header {
                pad_center_to_width(&line, content_width)
            } else {
                pad_to_width(&line, content_width)
            };
            Line::from(Span::styled(rendered, style))
        })
        .collect::<Vec<_>>();

    cell_from_rendered_content_lines(
        content_lines,
        content_width,
        is_first_column,
        show_left_separator,
        show_right_separator,
        draw_top_border,
        row_height,
        draw_bottom_border,
        draw_final_bottom_border,
        style,
    )
}

fn editor_cell_from_value(
    value: &str,
    width: usize,
    is_first_column: bool,
    show_left_separator: bool,
    show_right_separator: bool,
    non_wrapping: bool,
    row_height: usize,
    draw_final_bottom_border: bool,
    state: &TuiState,
) -> Cell<'static> {
    let content_width =
        width.saturating_sub(show_left_separator as usize + show_right_separator as usize);
    let style = Style::default().bg(Color::DarkGray);
    let content_lines =
        rendered_editor_cell_lines(value, content_width, non_wrapping, style, state);

    cell_from_rendered_content_lines(
        content_lines,
        content_width,
        is_first_column,
        show_left_separator,
        show_right_separator,
        false,
        row_height,
        true,
        draw_final_bottom_border,
        style,
    )
}

fn cell_from_rendered_content_lines(
    mut content_lines: Vec<Line<'static>>,
    content_width: usize,
    is_first_column: bool,
    show_left_separator: bool,
    show_right_separator: bool,
    draw_top_border: bool,
    row_height: usize,
    draw_bottom_border: bool,
    draw_final_bottom_border: bool,
    style: Style,
) -> Cell<'static> {
    let mut text_lines = Vec::new();

    if draw_top_border {
        text_lines.push(border_line(
            content_width,
            show_left_separator,
            show_right_separator,
            is_first_column,
            true,
            false,
        ));
    }

    let separator_style = separator_style();
    text_lines.extend(content_lines.drain(..).map(|line| {
        let mut spans = Vec::new();
        if show_left_separator {
            spans.push(Span::styled("│", separator_style));
        }
        spans.extend(line.spans);
        if show_right_separator {
            spans.push(Span::styled("│", separator_style));
        }
        Line::from(spans)
    }));

    while text_lines.len() < row_height {
        let mut spans = Vec::new();
        if show_left_separator {
            spans.push(Span::styled("│", separator_style));
        }
        spans.push(Span::styled(" ".repeat(content_width), style));
        if show_right_separator {
            spans.push(Span::styled("│", separator_style));
        }
        text_lines.push(Line::from(spans));
    }

    if draw_bottom_border {
        text_lines.push(border_line(
            content_width,
            show_left_separator,
            show_right_separator,
            is_first_column,
            false,
            draw_final_bottom_border,
        ));
    }

    Cell::from(text_lines)
}

fn border_line(
    content_width: usize,
    show_left_separator: bool,
    show_right_separator: bool,
    is_first_column: bool,
    is_top_border: bool,
    is_bottom_border: bool,
) -> Line<'static> {
    let mut spans = Vec::new();
    if show_left_separator {
        let left_joint = if is_top_border {
            if is_first_column { "┌" } else { "┬" }
        } else if is_bottom_border {
            if is_first_column { "└" } else { "┴" }
        } else if is_first_column {
            "├"
        } else {
            "┼"
        };
        spans.push(Span::styled(
            left_joint,
            Style::default().fg(Color::DarkGray),
        ));
    }
    spans.push(Span::styled(
        "─".repeat(content_width),
        Style::default().fg(Color::DarkGray),
    ));
    if show_right_separator {
        let right_joint = if is_top_border {
            "┐"
        } else if is_bottom_border {
            "┘"
        } else {
            "┤"
        };
        spans.push(Span::styled(
            right_joint,
            Style::default().fg(Color::DarkGray),
        ));
    }
    Line::from(spans)
}

#[derive(Default)]
struct EditorWrappedLine {
    chars: Vec<(usize, char)>,
    end_cursor: usize,
}

fn wrapped_editor_cell_lines(
    value: &str,
    width: usize,
    show_left_separator: bool,
    show_right_separator: bool,
    non_wrapping: bool,
) -> Vec<String> {
    let content_width =
        width.saturating_sub(show_left_separator as usize + show_right_separator as usize);
    if content_width == 0 {
        return vec![String::new()];
    }

    wrap_editor_lines(value, content_width, non_wrapping)
        .into_iter()
        .map(|line| line.chars.into_iter().map(|(_, ch)| ch).collect::<String>())
        .collect()
}

fn rendered_editor_cell_lines(
    value: &str,
    content_width: usize,
    non_wrapping: bool,
    base_style: Style,
    state: &TuiState,
) -> Vec<Line<'static>> {
    if content_width == 0 {
        return vec![Line::from("")];
    }

    let selection = state.cell_selection_range();
    let cursor = state.cell_cursor.min(value.chars().count());
    let show_cursor = state.show_cell_cursor();
    let selection_style = Style::default().fg(Color::Black).bg(Color::LightCyan);
    let cursor_style = Style::default().fg(Color::Black).bg(Color::White);

    wrap_editor_lines(value, content_width, non_wrapping)
        .into_iter()
        .map(|line| {
            let mut spans = Vec::new();
            let mut used = 0usize;

            for (index, ch) in line.chars {
                let mut style = base_style;
                if selection.is_some_and(|(start, end)| (start..end).contains(&index)) {
                    style = selection_style;
                }
                if show_cursor && cursor == index {
                    style = cursor_style;
                }
                spans.push(Span::styled(ch.to_string(), style));
                used += 1;
            }

            if show_cursor && cursor == line.end_cursor && used < content_width {
                spans.push(Span::styled(" ", cursor_style));
                used += 1;
            }

            if used < content_width {
                spans.push(Span::styled(" ".repeat(content_width - used), base_style));
            }

            Line::from(spans)
        })
        .collect()
}

fn wrap_editor_lines(
    value: &str,
    content_width: usize,
    non_wrapping: bool,
) -> Vec<EditorWrappedLine> {
    if content_width == 0 {
        return vec![EditorWrappedLine::default()];
    }

    let chars = value.chars().collect::<Vec<_>>();
    let mut lines = Vec::new();
    let mut current = EditorWrappedLine::default();

    for (index, ch) in chars.iter().copied().enumerate() {
        if ch == '\n' {
            current.end_cursor = index;
            lines.push(current);
            current = EditorWrappedLine {
                chars: Vec::new(),
                end_cursor: index + 1,
            };
            continue;
        }

        if non_wrapping && current.chars.len() >= content_width {
            continue;
        }

        if !non_wrapping && current.chars.len() >= content_width {
            current.end_cursor = index;
            lines.push(current);
            current = EditorWrappedLine {
                chars: Vec::new(),
                end_cursor: index,
            };
        }

        current.chars.push((index, ch));
        current.end_cursor = index + 1;
    }

    if lines.is_empty() || !current.chars.is_empty() || value.is_empty() || value.ends_with('\n') {
        current.end_cursor = current.end_cursor.max(chars.len());
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(EditorWrappedLine::default());
    }

    lines
}

fn wrapped_cell_lines(
    value: &str,
    width: usize,
    show_left_separator: bool,
    show_right_separator: bool,
    non_wrapping: bool,
) -> Vec<String> {
    let content_width =
        width.saturating_sub(show_left_separator as usize + show_right_separator as usize);
    if content_width == 0 {
        return vec![String::new()];
    }

    if non_wrapping {
        return vec![pad_or_clip(value, content_width)];
    }

    let mut lines = Vec::new();
    for original_line in value.lines() {
        lines.extend(wrap_text(original_line, content_width));
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn separator_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn table_widths(table: &SqlViewTable, inner_width: usize) -> Vec<usize> {
    let available_width = inner_width;
    let mut widths = table
        .headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            let border_width = 1 + usize::from(index == table.headers.len().saturating_sub(1));
            let mut content_width = header.chars().count();
            for row in &table.rows {
                content_width = content_width.max(
                    row.get(index)
                        .map(|value| value.chars().count())
                        .unwrap_or(0),
                );
            }
            let content_width = if is_non_wrapping_column(header) {
                content_width
            } else {
                content_width.min(32)
            };
            content_width + border_width
        })
        .collect::<Vec<_>>();

    fit_widths_to_area(&mut widths, &table.headers, available_width);
    widths
}

fn fit_widths_to_area(widths: &mut [usize], headers: &[String], inner_width: usize) {
    let base_min_content_width = if widths.len() <= 2 { 4 } else { 3 };
    let min_widths = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            let border_width = 1 + usize::from(index == headers.len().saturating_sub(1));
            if is_non_wrapping_column(header) {
                widths[index]
            } else {
                base_min_content_width + border_width
            }
        })
        .collect::<Vec<_>>();
    let min_total = min_widths.iter().sum::<usize>();

    if inner_width <= min_total {
        widths.copy_from_slice(&min_widths);
        return;
    }

    let mut total = widths.iter().sum::<usize>();
    while total > inner_width {
        let widest = widths
            .iter()
            .copied()
            .max()
            .unwrap_or(base_min_content_width);
        let candidates = widths
            .iter()
            .enumerate()
            .filter(|(index, width)| **width > min_widths[*index])
            .filter(|(_, width)| **width == widest)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            break;
        }
        for index in candidates {
            if total <= inner_width {
                break;
            }
            widths[index] -= 1;
            total -= 1;
        }
    }

    let expandable = headers
        .iter()
        .enumerate()
        .filter(|(_, header)| !is_non_wrapping_column(header))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let targets = if expandable.is_empty() {
        (0..widths.len()).collect::<Vec<_>>()
    } else {
        expandable
    };

    let mut remaining = inner_width.saturating_sub(widths.iter().sum::<usize>());
    let mut cursor = 0usize;
    while remaining > 0 && !targets.is_empty() {
        let index = targets[cursor % targets.len()];
        widths[index] += 1;
        remaining -= 1;
        cursor += 1;
    }
}

fn is_non_wrapping_column(header: &str) -> bool {
    let normalized = header.trim().to_ascii_lowercase();
    normalized == "id" || normalized == "quantity"
}

fn pad_or_clip(value: &str, width: usize) -> String {
    let clipped = value.chars().take(width).collect::<String>();
    pad_to_width(&clipped, width)
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
