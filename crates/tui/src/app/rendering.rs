use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    widgets::{Paragraph, Wrap},
};

use super::{
    components::{
        render_error_toasts, render_header, render_help, render_input, render_sql_table_lines,
    },
    logic::TuiState,
};

pub(super) fn render(frame: &mut Frame, state: &mut TuiState) {
    let areas = Layout::vertical([
        Constraint::Length(state.header_height()),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .split(frame.area());

    render_header(frame, areas[0], state);
    render_body(frame, areas[1], state);
    render_input(frame, areas[2], state);
    render_error_toasts(frame, state, areas[2]);

    if state.show_help {
        render_help(frame);
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

    let visible_rows = area.height as usize;
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

    let Some(sql_table) = selected_view.sql_table else {
        frame.render_widget(
            Paragraph::new("No SQL table data available for this view.").wrap(Wrap { trim: true }),
            area,
        );
        return;
    };

    let total_rows = sql_table.rows.len();
    let data_visible_rows = visible_rows.saturating_sub(2).max(1);
    state.last_visible_rows = data_visible_rows;
    state.clamp_scroll();
    let row_offset = state
        .row_offset
        .min(total_rows.saturating_sub(data_visible_rows));
    let inner_width = area.width as usize;
    let lines = render_sql_table_lines(
        &sql_table,
        row_offset,
        data_visible_rows,
        inner_width,
        state.selected_row,
        state.selected_col,
    );
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
