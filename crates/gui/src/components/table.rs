use gpui::*;
use gpui_component::{h_flex, scroll::ScrollableElement};
use injection::cross_cutting::InjectedServices;
use ropey::Rope;
use std::collections::{HashMap, HashSet};

use crate::components::modal_frame::{
    ModalConstraints, ModalFrameDrag, ModalInteraction, ModalRect, ResizeEdges, apply_drag,
    begin_drag_with_interaction,
};
use crate::components::table_vim::{EditMode, TableVimCommand, table_vim_command};
use crate::themes::catppuccin_mocha::{blue, surface1};

pub type Row = HashMap<String, String>;
pub type Table = Vec<Row>;

#[derive(Clone, Debug, PartialEq)]
struct CellPosition {
    row_ix: usize,
    col_ix: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditSurface {
    Inline,
    Modal,
}

pub struct CustomTable {
    data: Table,
    headers: Vec<String>,
    table_name: String,
    services: InjectedServices,
    editing_cell: Option<CellPosition>,
    hovered_cell: Option<CellPosition>,
    edit_value: Rope,
    cursor_pos: usize,
    preferred_col: Option<usize>,
    edit_mode: EditMode,
    edit_surface: EditSurface,
    col_widths: Vec<f32>,
    resizing_col: Option<(usize, f32, f32)>,
    modal_width: f32,
    modal_height: f32,
    modal_x: f32,
    modal_y: f32,
    modal_drag: Option<ModalFrameDrag>,
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
        let header_count = headers.len();

        Self {
            data,
            headers,
            table_name,
            services,
            editing_cell: None,
            hovered_cell: None,
            edit_value: Rope::new(),
            cursor_pos: 0,
            preferred_col: None,
            edit_mode: EditMode::Normal,
            edit_surface: EditSurface::Inline,
            col_widths: vec![180.0; header_count],
            resizing_col: None,
            modal_width: 1200.0,
            modal_height: 760.0,
            modal_x: 2400.0,
            modal_y: 2200.0,
            modal_drag: None,
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

    fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_'
    }

    fn move_word_forward(&mut self) {
        self.clamp_cursor();
        let chars = self.edit_value.to_string().chars().collect::<Vec<_>>();
        let len = chars.len();
        if self.cursor_pos >= len {
            return;
        }
        let mut i = self.cursor_pos;
        if i < len && Self::is_word_char(chars[i]) {
            while i < len && Self::is_word_char(chars[i]) {
                i += 1;
            }
        }
        while i < len && !Self::is_word_char(chars[i]) {
            i += 1;
        }
        self.cursor_pos = i.min(len);
        self.preferred_col = None;
    }

    fn move_word_backward(&mut self) {
        self.clamp_cursor();
        let chars = self.edit_value.to_string().chars().collect::<Vec<_>>();
        if self.cursor_pos == 0 || chars.is_empty() {
            return;
        }
        let mut i = self.cursor_pos.saturating_sub(1);
        while i > 0 && !Self::is_word_char(chars[i]) {
            i -= 1;
        }
        while i > 0 && Self::is_word_char(chars[i - 1]) {
            i -= 1;
        }
        self.cursor_pos = i;
        self.preferred_col = None;
    }

    fn editor_lines_with_cursor(&self) -> Vec<String> {
        self.edit_value
            .to_string()
            .split('\n')
            .map(str::to_string)
            .collect::<Vec<_>>()
    }

    fn cursor_line_col(&self) -> (usize, usize) {
        let edit_text = self.edit_value.to_string();
        let text_len = edit_text.chars().count();
        let cursor = self.cursor_pos.min(text_len);
        let starts = Self::line_starts(&edit_text);
        Self::line_index_and_col(cursor, &starts)
    }

    fn editor_line_element(
        &self,
        line: String,
        line_ix: usize,
        cursor_line: usize,
        cursor_col: usize,
    ) -> AnyElement {
        if line_ix != cursor_line {
            return div().w_full().child(line).into_any_element();
        }

        let chars = line.chars().collect::<Vec<_>>();
        match self.edit_mode {
            EditMode::Insert => {
                let split_ix = cursor_col.min(chars.len());
                h_flex()
                    .w_full()
                    .min_w(px(0.0))
                    .items_center()
                    .flex_wrap()
                    .whitespace_normal()
                    .children(
                        chars
                            .iter()
                            .take(split_ix)
                            .map(|ch| div().child(ch.to_string()).into_any_element()),
                    )
                    .child(
                        div()
                            .w(px(1.0))
                            .h(px(18.0))
                            .bg(rgb(0xffffff))
                            .flex_shrink_0(),
                    )
                    .children(
                        chars
                            .iter()
                            .skip(split_ix)
                            .map(|ch| div().child(ch.to_string()).into_any_element()),
                    )
                    .into_any_element()
            }
            EditMode::Normal => {
                if chars.is_empty() {
                    return h_flex()
                        .w_full()
                        .items_center()
                        .child(div().bg(rgb(0xffffff)).text_color(rgb(0x000000)).child(" "))
                        .into_any_element();
                }
                let ix = cursor_col.min(chars.len().saturating_sub(1));
                h_flex()
                    .w_full()
                    .min_w(px(0.0))
                    .items_center()
                    .flex_wrap()
                    .whitespace_normal()
                    .children(
                        chars
                            .iter()
                            .take(ix)
                            .map(|ch| div().child(ch.to_string()).into_any_element()),
                    )
                    .child(
                        div()
                            .bg(rgb(0xffffff))
                            .text_color(rgb(0x000000))
                            .child(chars[ix].to_string()),
                    )
                    .children(
                        chars
                            .iter()
                            .skip(ix + 1)
                            .map(|ch| div().child(ch.to_string()).into_any_element()),
                    )
                    .into_any_element()
            }
        }
    }

    fn modal_rect(&self) -> ModalRect {
        ModalRect {
            x: self.modal_x,
            y: self.modal_y,
            width: self.modal_width,
            height: self.modal_height,
        }
    }

    fn begin_modal_interaction(
        &mut self,
        event: &MouseDownEvent,
        interaction: ModalInteraction,
        cx: &mut Context<Self>,
    ) {
        let x = f32::from(event.position.x);
        let y = f32::from(event.position.y);
        let rect = self.modal_rect();
        self.modal_drag = Some(begin_drag_with_interaction(rect, x, y, interaction));
        cx.notify();
    }

    fn update_modal_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !event.dragging() {
            return;
        }
        let Some(drag) = self.modal_drag else {
            return;
        };
        let rect = apply_drag(
            drag,
            f32::from(event.position.x),
            f32::from(event.position.y),
            ModalConstraints {
                min_width: 700.0,
                min_height: 420.0,
                max_width: 1800.0,
                max_height: 1200.0,
            },
        );
        self.modal_x = rect.x;
        self.modal_y = rect.y;
        self.modal_width = rect.width;
        self.modal_height = rect.height;
        cx.notify();
    }

    fn end_modal_drag(&mut self, cx: &mut Context<Self>) {
        if self.modal_drag.is_some() {
            self.modal_drag = None;
            cx.notify();
        }
    }

    fn begin_column_resize(
        &mut self,
        col_ix: usize,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let start_x = f32::from(event.position.x);
        let start_width = self.col_widths.get(col_ix).copied().unwrap_or(180.0);
        self.resizing_col = Some((col_ix, start_x, start_width));
        cx.notify();
    }

    fn update_column_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !event.dragging() {
            return;
        }
        let Some((col_ix, start_x, start_width)) = self.resizing_col else {
            return;
        };
        let current_x = f32::from(event.position.x);
        let width = (start_width + (current_x - start_x)).clamp(80.0, 900.0);
        if let Some(w) = self.col_widths.get_mut(col_ix) {
            *w = width;
            cx.notify();
        }
    }

    fn end_column_resize(&mut self, cx: &mut Context<Self>) {
        if self.resizing_col.is_some() {
            self.resizing_col = None;
            cx.notify();
        }
    }

    pub fn editing_mode_widget_label(&self, window: &Window) -> Option<&'static str> {
        if self.editing_cell.is_some() && self.focus_handle.is_focused(window) {
            Some(match self.edit_mode {
                EditMode::Normal => "N",
                EditMode::Insert => "I",
            })
        } else {
            None
        }
    }

    fn current_edit_row_meta(&self) -> (String, String) {
        let Some(pos) = &self.editing_cell else {
            return ("-".to_string(), "-".to_string());
        };
        let Some(row) = self.data.get(pos.row_ix) else {
            return ("-".to_string(), "-".to_string());
        };
        let id = row
            .get("id")
            .cloned()
            .unwrap_or_else(|| pos.row_ix.to_string());
        let quantity = row
            .get("quantity")
            .cloned()
            .unwrap_or_else(|| "-".to_string());
        (id, quantity)
    }

    fn handle_table_vim_command(&mut self, command: TableVimCommand, cx: &mut Context<Self>) {
        match command {
            TableVimCommand::None => {}
            TableVimCommand::Save => self.finish_edit(cx),
            TableVimCommand::CancelEdit => self.cancel_edit(cx),
            TableVimCommand::EnterInsertMode => {
                self.edit_mode = EditMode::Insert;
                cx.notify();
            }
            TableVimCommand::EnterNormalMode => {
                self.edit_mode = EditMode::Normal;
                cx.notify();
            }
            TableVimCommand::MoveLeft => {
                self.move_left();
                cx.notify();
            }
            TableVimCommand::MoveDown => {
                self.move_down();
                cx.notify();
            }
            TableVimCommand::MoveUp => {
                self.move_up();
                cx.notify();
            }
            TableVimCommand::MoveRight => {
                self.move_right();
                cx.notify();
            }
            TableVimCommand::MoveHome => {
                self.move_home();
                cx.notify();
            }
            TableVimCommand::MoveEnd => {
                self.move_end();
                cx.notify();
            }
            TableVimCommand::MoveWordBackward => {
                self.move_word_backward();
                cx.notify();
            }
            TableVimCommand::MoveWordForward => {
                self.move_word_forward();
                cx.notify();
            }
            TableVimCommand::DeleteBackward => {
                self.backspace();
                cx.notify();
            }
            TableVimCommand::DeleteForward => {
                self.delete_forward();
                cx.notify();
            }
            TableVimCommand::InsertNewline => {
                self.insert_text("\n");
                cx.notify();
            }
            TableVimCommand::InsertChar(ch) => {
                let mut encoded = [0; 4];
                self.insert_text(ch.encode_utf8(&mut encoded));
                cx.notify();
            }
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
        surface: EditSurface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &self.headers[col_ix];
        let value = self.data[row_ix].get(key).map(String::as_str).unwrap_or("");
        self.editing_cell = Some(CellPosition { row_ix, col_ix });
        self.edit_value = Rope::from_str(value);
        self.cursor_pos = self.edit_value.len_chars();
        self.preferred_col = None;
        self.edit_mode = EditMode::Normal;
        self.edit_surface = surface;
        if self.edit_surface == EditSurface::Modal {
            let viewport = window.viewport_size();
            let viewport_width = f32::from(viewport.width);
            let viewport_height = f32::from(viewport.height);
            self.modal_x = ((viewport_width - self.modal_width) * 0.5).max(0.0);
            self.modal_y = ((viewport_height - self.modal_height) * 0.5).max(0.0);
        }
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
        self.edit_mode = EditMode::Normal;
        self.edit_surface = EditSurface::Inline;
        self.modal_drag = None;
        cx.notify();
    }

    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.editing_cell = None;
        self.preferred_col = None;
        self.edit_mode = EditMode::Normal;
        self.edit_surface = EditSurface::Inline;
        self.modal_drag = None;
        cx.notify();
    }
}

impl Render for CustomTable {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header_cells = self.headers.iter().enumerate().map(|(col_ix, header)| {
            let width = self.col_widths.get(col_ix).copied().unwrap_or(180.0);
            div()
                .relative()
                .w(px(width))
                .flex_shrink_0()
                .p_1p5()
                .bg(rgba(0xffffff08))
                .border_r_1()
                .border_color(rgba(0xffffff14))
                .child(header.clone())
                .child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        .right(px(0.0))
                        .h_full()
                        .w(px(8.0))
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                this.begin_column_resize(col_ix, event, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
        });

        let column_separator_positions = self
            .headers
            .iter()
            .enumerate()
            .scan(0.0f32, |x, (col_ix, _)| {
                let width = self.col_widths.get(col_ix).copied().unwrap_or(180.0);
                *x += width;
                Some(*x)
            })
            .take(self.headers.len().saturating_sub(1))
            .collect::<Vec<_>>();

        let rows = self.data.iter().enumerate().map(|(row_ix, row)| {
            let cells = self.headers.iter().enumerate().map(|(col_ix, key)| {
                let value = row.get(key).map(String::as_str).unwrap_or("NULL");
                let lines = value.split('\n').map(str::to_string).collect::<Vec<_>>();
                let current_pos = CellPosition { row_ix, col_ix };
                let is_editing_inline = self.editing_cell.as_ref() == Some(&current_pos)
                    && self.edit_surface == EditSurface::Inline;
                let cell_id = row_ix
                    .saturating_mul(self.headers.len())
                    .saturating_add(col_ix);

                if is_editing_inline {
                    let edit_lines = self.editor_lines_with_cursor();
                    let (cursor_line, cursor_col) = self.cursor_line_col();
                    let width = self.col_widths.get(col_ix).copied().unwrap_or(180.0);
                    div()
                        .id(("cell_editor", cell_id))
                        .w(px(width))
                        .h_full()
                        .flex_shrink_0()
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
                            let command = table_vim_command(event, this.edit_mode);
                            if command != TableVimCommand::None {
                                this.handle_table_vim_command(command, cx);
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        }))
                        .children(edit_lines.into_iter().enumerate().map(|(line_ix, line)| {
                            self.editor_line_element(line, line_ix, cursor_line, cursor_col)
                        }))
                        .into_any_element()
                } else {
                    let show_modal_icon = self.hovered_cell.as_ref() == Some(&current_pos);
                    let width = self.col_widths.get(col_ix).copied().unwrap_or(180.0);
                    div()
                        .id(("cell_view", cell_id))
                        .relative()
                        .w(px(width))
                        .h_full()
                        .flex_shrink_0()
                        .p_1p5()
                        .whitespace_normal()
                        .flex_col()
                        .cursor_pointer()
                        .hover(|style| style.bg(rgba(0xffffff11)))
                        .on_hover(cx.listener(move |this, hovered, _window, cx| {
                            let hovered_pos = CellPosition { row_ix, col_ix };
                            if *hovered {
                                this.hovered_cell = Some(hovered_pos);
                            } else if this.hovered_cell.as_ref() == Some(&hovered_pos) {
                                this.hovered_cell = None;
                            }
                            cx.notify();
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, window, cx| {
                                this.start_edit(row_ix, col_ix, EditSurface::Inline, window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .children(if show_modal_icon {
                            Some(
                                div()
                                    .absolute()
                                    .top_1()
                                    .right_1()
                                    .px_1()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(rgba(0xffffff1a))
                                    .hover(|s| s.bg(rgba(0xffffff2a)))
                                    .text_xs()
                                    .child("✎")
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, window, cx| {
                                            this.start_edit(
                                                row_ix,
                                                col_ix,
                                                EditSurface::Modal,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                        } else {
                            None
                        })
                        .children(lines.into_iter().map(|line| div().w_full().child(line)))
                        .into_any_element()
                }
            });

            div()
                .flex()
                .flex_row()
                .relative()
                .w_full()
                .min_w(px(0.0))
                .border_b_1()
                .border_color(rgba(0xffffff14))
                .children(cells)
                .children(column_separator_positions.iter().map(|x| {
                    div()
                        .absolute()
                        .left(px(*x))
                        .top_0()
                        .bottom_0()
                        .w(px(1.0))
                        .bg(rgba(0xffffff14))
                }))
        });

        let modal_overlay = if self.edit_surface == EditSurface::Modal
            && self.editing_cell.is_some()
        {
            let edit_lines = self.editor_lines_with_cursor();
            let (cursor_line, cursor_col) = self.cursor_line_col();
            let (row_id, row_quantity) = self.current_edit_row_meta();
            Some(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .size_full()
                    .bg(rgba(0x00000099))
                    .flex()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.update_modal_drag(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                            this.end_modal_drag(cx);
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(self.modal_x))
                            .top(px(self.modal_y))
                            .w(px(self.modal_width))
                            .h(px(self.modal_height))
                            .bg(surface1())
                            .border_1()
                            .border_color(blue())
                            .relative()
                            .rounded_lg()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .top(px(0.0))
                                    .w(px(10.0))
                                    .h_full()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: true,
                                                    right: false,
                                                    top: false,
                                                    bottom: false,
                                                }),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .right(px(0.0))
                                    .top(px(0.0))
                                    .w(px(10.0))
                                    .h_full()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: true,
                                                    top: false,
                                                    bottom: false,
                                                }),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .top(px(0.0))
                                    .w_full()
                                    .h(px(10.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: false,
                                                    top: true,
                                                    bottom: false,
                                                }),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .bottom(px(0.0))
                                    .w_full()
                                    .h(px(10.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: false,
                                                    top: false,
                                                    bottom: true,
                                                }),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .top(px(0.0))
                                    .w(px(14.0))
                                    .h(px(14.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: true,
                                                    right: false,
                                                    top: true,
                                                    bottom: false,
                                                }),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .right(px(0.0))
                                    .top(px(0.0))
                                    .w(px(14.0))
                                    .h(px(14.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: true,
                                                    top: true,
                                                    bottom: false,
                                                }),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .bottom(px(0.0))
                                    .w(px(14.0))
                                    .h(px(14.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: true,
                                                    right: false,
                                                    top: false,
                                                    bottom: true,
                                                }),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .right(px(0.0))
                                    .bottom(px(0.0))
                                    .w(px(14.0))
                                    .h(px(14.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Resize(ResizeEdges {
                                                    left: false,
                                                    right: true,
                                                    top: false,
                                                    bottom: true,
                                                }),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .items_center()
                                    .justify_between()
                                    .p_2()
                                    .border_b_2()
                                    .border_color(rgba(0xffffff1f))
                                    .child(format!("Id: {row_id}, Quantity: {row_quantity}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                            this.begin_modal_interaction(
                                                event,
                                                ModalInteraction::Move,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("modal_editor")
                                    .w_full()
                                    .h_full()
                                    .p_3()
                                    .overflow_y_hidden()
                                    .flex_col()
                                    .focusable()
                                    .track_focus(&self.focus_handle(cx))
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, window, cx| {
                                            let command = table_vim_command(event, this.edit_mode);
                                            if command != TableVimCommand::None {
                                                this.handle_table_vim_command(command, cx);
                                                window.prevent_default();
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .children(edit_lines.into_iter().enumerate().map(
                                        |(line_ix, line)| {
                                            self.editor_line_element(
                                                line,
                                                line_ix,
                                                cursor_line,
                                                cursor_col,
                                            )
                                        },
                                    )),
                            ),
                    ),
            )
        } else {
            None
        };

        div()
            .relative()
            .w_full()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                this.update_column_resize(event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.end_column_resize(cx);
                }),
            )
            .child(
                div()
                    .w_full()
                    .overflow_x_scrollbar()
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
                    .children(rows),
            )
            .children(modal_overlay)
    }
}
