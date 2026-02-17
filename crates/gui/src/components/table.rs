use gpui::*;
use gpui_component::h_flex;
use injection::cross_cutting::InjectedServices;
use ropey::Rope;
use std::collections::{HashMap, HashSet};

use crate::themes::catppuccin_mocha::{blue, surface1};

pub type Row = HashMap<String, String>;
pub type Table = Vec<Row>;

#[derive(Clone, Debug, PartialEq)]
struct CellPosition {
    row_ix: usize,
    col_ix: usize,
}

pub struct CustomTable {
    data: Table,
    headers: Vec<String>,
    table_name: String,
    services: InjectedServices,
    editing_cell: Option<CellPosition>,
    edit_value: Rope,
    cursor_pos: usize,
    preferred_col: Option<usize>,
    focus_handle: FocusHandle,
}

impl Focusable for CustomTable {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl CustomTable {
    pub fn new(data: Table, table_name: String, services: InjectedServices, cx: &mut App) -> Self {
        let mut header_set = HashSet::new();
        for row in &data {
            for key in row.keys() {
                header_set.insert(key.clone());
            }
        }
        let mut headers = header_set.into_iter().collect::<Vec<_>>();
        headers.sort();

        Self {
            data,
            headers,
            table_name,
            services,
            editing_cell: None,
            edit_value: Rope::new(),
            cursor_pos: 0,
            preferred_col: None,
            focus_handle: cx.focus_handle(),
        }
    }

    fn line_starts(text: &str) -> Vec<usize> {
        let mut starts = vec![0];
        for (ix, ch) in text.chars().enumerate() {
            if ch == '\n' {
                starts.push(ix + 1);
            }
        }
        starts
    }

    fn line_index_and_col(cursor: usize, starts: &[usize]) -> (usize, usize) {
        let line_ix = starts.partition_point(|s| *s <= cursor).saturating_sub(1);
        let col = cursor.saturating_sub(starts[line_ix]);
        (line_ix, col)
    }

    fn line_end(starts: &[usize], text_len: usize, line_ix: usize) -> usize {
        if line_ix + 1 < starts.len() {
            starts[line_ix + 1].saturating_sub(1)
        } else {
            text_len
        }
    }

    fn line_col_to_cursor(
        starts: &[usize],
        text_len: usize,
        line_ix: usize,
        target_col: usize,
    ) -> usize {
        let line_start = starts[line_ix];
        let line_end = Self::line_end(starts, text_len, line_ix);
        line_start + target_col.min(line_end.saturating_sub(line_start))
    }

    fn clamp_cursor(&mut self) {
        let len = self.edit_value.len_chars();
        if self.cursor_pos > len {
            self.cursor_pos = len;
        }
    }

    fn insert_text(&mut self, text: &str) {
        self.clamp_cursor();
        self.edit_value.insert(self.cursor_pos, text);
        self.cursor_pos += text.chars().count();
        self.preferred_col = None;
    }

    fn move_left(&mut self) {
        self.clamp_cursor();
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
        self.preferred_col = None;
    }

    fn move_right(&mut self) {
        self.clamp_cursor();
        if self.cursor_pos < self.edit_value.len_chars() {
            self.cursor_pos += 1;
        }
        self.preferred_col = None;
    }

    fn move_home(&mut self) {
        self.clamp_cursor();
        let text = self.edit_value.to_string();
        let starts = Self::line_starts(&text);
        let (line_ix, _) = Self::line_index_and_col(self.cursor_pos, &starts);
        self.cursor_pos = starts[line_ix];
        self.preferred_col = None;
    }

    fn move_end(&mut self) {
        self.clamp_cursor();
        let text = self.edit_value.to_string();
        let text_len = text.chars().count();
        let starts = Self::line_starts(&text);
        let (line_ix, _) = Self::line_index_and_col(self.cursor_pos, &starts);
        self.cursor_pos = Self::line_end(&starts, text_len, line_ix);
        self.preferred_col = None;
    }

    fn move_up(&mut self) {
        self.clamp_cursor();
        let text = self.edit_value.to_string();
        let text_len = text.chars().count();
        let starts = Self::line_starts(&text);
        let (line_ix, col) = Self::line_index_and_col(self.cursor_pos, &starts);
        let target_col = self.preferred_col.unwrap_or(col);
        if line_ix > 0 {
            self.cursor_pos = Self::line_col_to_cursor(&starts, text_len, line_ix - 1, target_col);
        }
        self.preferred_col = Some(target_col);
    }

    fn move_down(&mut self) {
        self.clamp_cursor();
        let text = self.edit_value.to_string();
        let text_len = text.chars().count();
        let starts = Self::line_starts(&text);
        let (line_ix, col) = Self::line_index_and_col(self.cursor_pos, &starts);
        let target_col = self.preferred_col.unwrap_or(col);
        if line_ix + 1 < starts.len() {
            self.cursor_pos = Self::line_col_to_cursor(&starts, text_len, line_ix + 1, target_col);
        }
        self.preferred_col = Some(target_col);
    }

    fn backspace(&mut self) {
        self.clamp_cursor();
        if self.cursor_pos > 0 {
            let start = self.cursor_pos - 1;
            self.edit_value.remove(start..self.cursor_pos);
            self.cursor_pos = start;
            self.preferred_col = None;
        }
    }

    fn delete_forward(&mut self) {
        self.clamp_cursor();
        if self.cursor_pos < self.edit_value.len_chars() {
            self.edit_value.remove(self.cursor_pos..self.cursor_pos + 1);
            self.preferred_col = None;
        }
    }

    fn save_cell_changes(&mut self, row_ix: usize, col_ix: usize, cx: &mut Context<Self>) {
        let key = self.headers[col_ix].clone();
        let row_id = self.data[row_ix]
            .get("id")
            .map(|s| s.to_string())
            .unwrap_or_else(|| row_ix.to_string());
        let services = self.services.clone();
        let table = self.table_name.clone();
        let column = key.clone();
        let value = self.edit_value.to_string();

        if let Some(row) = self.data.get_mut(row_ix) {
            row.insert(key, value.clone());
        }

        cx.spawn(async move |_this, _cx| {
            if let Err(e) =
                application::table::table_patch_row(services, table, row_id, column, value).await
            {
                eprintln!("Failed to save cell: {}", e);
            }
        })
        .detach();
    }

    fn start_edit(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &self.headers[col_ix];
        let value = self.data[row_ix].get(key).map(String::as_str).unwrap_or("");
        self.editing_cell = Some(CellPosition { row_ix, col_ix });
        self.edit_value = Rope::from_str(value);
        self.cursor_pos = self.edit_value.len_chars();
        self.preferred_col = None;
        window.defer(cx, {
            let focus_handle = self.focus_handle.clone();
            move |window, _cx| {
                window.focus(&focus_handle);
            }
        });
        cx.notify();
    }

    fn finish_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(pos) = self.editing_cell.clone() {
            self.save_cell_changes(pos.row_ix, pos.col_ix, cx);
        }
        self.editing_cell = None;
        self.preferred_col = None;
        cx.notify();
    }

    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.editing_cell = None;
        self.preferred_col = None;
        cx.notify();
    }
}

impl Render for CustomTable {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header_cells = self.headers.iter().map(|header| {
            div()
                .min_w(px(120.0))
                .flex_1()
                .p_1p5()
                .bg(rgba(0xffffff08))
                .border_r_1()
                .border_color(rgba(0xffffff14))
                .child(header.clone())
        });

        let rows = self.data.iter().enumerate().map(|(row_ix, row)| {
            let cells = self.headers.iter().enumerate().map(|(col_ix, key)| {
                let value = row.get(key).map(String::as_str).unwrap_or("NULL");
                let lines = value.split('\n').map(str::to_string).collect::<Vec<_>>();
                let current_pos = CellPosition { row_ix, col_ix };
                let is_editing = self.editing_cell.as_ref() == Some(&current_pos);
                let cell_id = row_ix
                    .saturating_mul(self.headers.len())
                    .saturating_add(col_ix);

                if is_editing {
                    let edit_text = self.edit_value.to_string();
                    let text_len = edit_text.chars().count();
                    let cursor = self.cursor_pos.min(text_len);
                    let starts = Self::line_starts(&edit_text);
                    let (cursor_line, cursor_col) = Self::line_index_and_col(cursor, &starts);
                    let edit_lines = edit_text
                        .split('\n')
                        .enumerate()
                        .map(|(line_ix, line)| {
                            if line_ix == cursor_line {
                                let chars = line.chars().collect::<Vec<_>>();
                                let split_ix = cursor_col.min(chars.len());
                                let before = chars.iter().take(split_ix).collect::<String>();
                                let after = chars.iter().skip(split_ix).collect::<String>();
                                format!("{before}|{after}")
                            } else {
                                line.to_string()
                            }
                        })
                        .collect::<Vec<_>>();
                    div()
                        .id(("cell_editor", cell_id))
                        .min_w(px(120.0))
                        .flex_1()
                        .p_1p5()
                        .bg(surface1())
                        .border_1()
                        .border_color(blue())
                        .whitespace_normal()
                        .flex_col()
                        .focusable()
                        .track_focus(&self.focus_handle(cx))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _event, _window, cx| {
                                cx.stop_propagation();
                            }),
                        )
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                            match event.keystroke.key.as_str() {
                                "enter" => {
                                    if event.keystroke.modifiers.shift {
                                        this.insert_text("\n");
                                        cx.notify();
                                    } else {
                                        this.finish_edit(cx);
                                    }
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "escape" => {
                                    this.cancel_edit(cx);
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "backspace" => {
                                    this.backspace();
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "delete" => {
                                    this.delete_forward();
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "left" | "arrowleft" => {
                                    this.move_left();
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "right" | "arrowright" => {
                                    this.move_right();
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "up" | "arrowup" => {
                                    this.move_up();
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "down" | "arrowdown" => {
                                    this.move_down();
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "home" => {
                                    this.move_home();
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                "end" => {
                                    this.move_end();
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    return;
                                }
                                _ => {}
                            }

                            if let Some(key_char) = &event.keystroke.key_char {
                                if key_char.chars().count() == 1
                                    && !event.keystroke.modifiers.control
                                    && !event.keystroke.modifiers.platform
                                {
                                    this.insert_text(key_char);
                                    cx.notify();
                                    window.prevent_default();
                                    cx.stop_propagation();
                                }
                            }
                        }))
                        .children(
                            edit_lines
                                .into_iter()
                                .map(|line| div().w_full().child(line)),
                        )
                        .into_any_element()
                } else {
                    div()
                        .id(("cell_view", cell_id))
                        .min_w(px(120.0))
                        .flex_1()
                        .p_1p5()
                        .whitespace_normal()
                        .flex_col()
                        .cursor_pointer()
                        .border_r_1()
                        .border_color(rgba(0xffffff14))
                        .hover(|style| style.bg(rgba(0xffffff11)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, window, cx| {
                                this.start_edit(row_ix, col_ix, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .children(lines.into_iter().map(|line| div().w_full().child(line)))
                        .into_any_element()
                }
            });

            h_flex()
                .w_full()
                .border_b_1()
                .border_color(rgba(0xffffff14))
                .children(cells)
        });

        div()
            .w_full()
            .overflow_hidden()
            .border_1()
            .border_color(rgba(0xffffff14))
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .border_b_1()
                    .border_color(rgba(0xffffff14))
                    .children(header_cells),
            )
            .children(rows)
    }
}
