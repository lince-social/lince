use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::{
    components::{
        render_create_shortcut_picker, render_creation_modal, render_error_toasts, render_header,
        render_help, render_input, render_sql_table, sql_table_row_heights,
    },
    logic::TuiState,
};

pub(super) fn render(frame: &mut Frame, state: &mut TuiState) {
    let area = frame.area().inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let areas = Layout::vertical([
        Constraint::Length(state.header_height()),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .split(area);

    let body_area = ratatui::layout::Rect {
        height: areas[1].height.saturating_sub(1),
        ..areas[1]
    };

    render_header(frame, areas[0], state);
    render_body(frame, body_area, state);
    render_input(frame, areas[2], state);
    render_create_shortcut_picker(frame, state, areas[2]);
    render_error_toasts(frame, state, areas[2]);

    if state.show_help {
        render_help(frame);
    }

    if let Some(modal) = state.creation_modal() {
        render_creation_modal(frame, modal);
    }
}

fn render_body(frame: &mut Frame, area: ratatui::layout::Rect, state: &mut TuiState) {
    if state.active_collection().is_none() {
        frame.render_widget(Paragraph::new("No active collection selected"), area);
        return;
    }

    if state.active_views().is_empty() {
        frame.render_widget(
            Paragraph::new("This collection has no normal SQL views to render.")
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let Some(selected_view) = state.selected_view().cloned() else {
        return;
    };

    if selected_view.quantity != 1 {
        frame.render_widget(
            Paragraph::new("This view is disabled. Press Alt+Space to toggle it to 1.")
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let Some(sql_table) = state.rendered_selected_view_table() else {
        frame.render_widget(
            Paragraph::new("No SQL table data available for this view.").wrap(Wrap { trim: true }),
            area,
        );
        return;
    };

    let body_areas = Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).split(area);
    let table_area = body_areas[0];
    let scrollbar_area = body_areas[1];
    let inner_width = table_area.width as usize;
    let row_heights = sql_table_row_heights(&sql_table, state, inner_width);
    let visible_line_capacity = table_area.height.saturating_sub(3) as usize;
    let row_offset = visual_row_offset(
        &row_heights,
        state.row_offset,
        state.selected_row,
        visible_line_capacity,
    );
    state.row_offset = row_offset;
    state.last_visible_rows = visible_row_count(&row_heights, row_offset, visible_line_capacity);

    render_sql_table(
        frame,
        table_area,
        &sql_table,
        state,
        inner_width,
        row_offset,
    );

    render_vertical_scrollbar(
        frame,
        scrollbar_area,
        &row_heights,
        row_offset,
        visible_line_capacity,
    );
}

fn visual_row_offset(
    row_heights: &[usize],
    requested_offset: usize,
    selected_row: usize,
    visible_line_capacity: usize,
) -> usize {
    if row_heights.is_empty() || visible_line_capacity == 0 {
        return 0;
    }

    let bottom_aligned_offset = bottom_aligned_row_offset(row_heights, visible_line_capacity);
    let remaining_from_selected =
        visual_height(&row_heights[selected_row.min(row_heights.len().saturating_sub(1))..]);
    if selected_row >= bottom_aligned_offset || remaining_from_selected <= visible_line_capacity {
        return bottom_aligned_offset;
    }

    let mut offset = requested_offset.min(row_heights.len().saturating_sub(1));
    if selected_row < offset {
        offset = selected_row;
    }

    while selected_row < row_heights.len()
        && visual_height(&row_heights[offset..=selected_row]) > visible_line_capacity
        && offset < selected_row
    {
        offset += 1;
    }

    offset = offset.min(bottom_aligned_offset);

    offset
}

fn visible_row_count(
    row_heights: &[usize],
    row_offset: usize,
    visible_line_capacity: usize,
) -> usize {
    if visible_line_capacity == 0 || row_offset >= row_heights.len() {
        return 0;
    }

    let mut used = 0usize;
    let mut count = 0usize;
    for height in &row_heights[row_offset..] {
        if count > 0 && used + height > visible_line_capacity {
            break;
        }
        if *height > visible_line_capacity && count == 0 {
            return 1;
        }
        if used + height > visible_line_capacity {
            break;
        }
        used += height;
        count += 1;
    }
    count.max(1)
}

fn visual_height(row_heights: &[usize]) -> usize {
    row_heights.iter().sum()
}

fn bottom_aligned_row_offset(row_heights: &[usize], visible_line_capacity: usize) -> usize {
    if row_heights.is_empty() || visible_line_capacity == 0 {
        return 0;
    }

    let mut used = 0usize;
    let mut offset = row_heights.len().saturating_sub(1);

    for index in (0..row_heights.len()).rev() {
        if used + row_heights[index] > visible_line_capacity && used > 0 {
            break;
        }
        used += row_heights[index];
        offset = index;
        if used >= visible_line_capacity {
            break;
        }
    }

    offset
}

fn render_vertical_scrollbar(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    row_heights: &[usize],
    row_offset: usize,
    visible_line_capacity: usize,
) {
    if area.width == 0 || area.height == 0 || row_heights.is_empty() {
        return;
    }

    let content_length = visual_height(row_heights);
    if content_length <= visible_line_capacity {
        return;
    }

    let max_row_offset = bottom_aligned_row_offset(row_heights, visible_line_capacity);
    let max_scroll = content_length.saturating_sub(visible_line_capacity);
    let remaining_from_offset = visual_height(&row_heights[row_offset.min(row_heights.len())..]);
    let position = if row_offset >= max_row_offset || remaining_from_offset <= visible_line_capacity
    {
        max_scroll
    } else {
        visual_height(&row_heights[..row_offset.min(row_heights.len())]).min(max_scroll)
    };
    let track_height = area.height as usize;
    let thumb_height = ((visible_line_capacity * track_height) / content_length)
        .max(1)
        .min(track_height);
    let max_thumb_offset = track_height.saturating_sub(thumb_height);
    let thumb_offset = if max_scroll == 0 || max_thumb_offset == 0 {
        0
    } else {
        position.saturating_mul(max_thumb_offset) / max_scroll
    };

    let lines = (0..track_height)
        .map(|index| {
            let style = if (thumb_offset..thumb_offset + thumb_height).contains(&index) {
                Style::default().bg(Color::Gray)
            } else {
                Style::default().bg(Color::DarkGray)
            };
            Line::from(Span::styled(" ", style))
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(lines), area);
}
