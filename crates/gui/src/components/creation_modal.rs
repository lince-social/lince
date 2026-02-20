use std::collections::HashMap;

use domain::dirty::operation::OperationTables;
use gpui::{
    App, Context, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Render, StatefulInteractiveElement, Styled,
    WeakEntity, Window, div, px,
};
use gpui_component::scroll::ScrollableElement;

use crate::{
    themes::catppuccin_mocha::{blue, crust, green, mantle, red, surface0, surface1, text, yellow},
    workspace::Workspace,
};

pub struct CreationModal {
    workspace: WeakEntity<Workspace>,
    table: OperationTables,
    table_name: String,
    columns: Vec<String>,
    values: Vec<String>,
    cursors: Vec<usize>,
    active_field_ix: usize,
    focus_handle: FocusHandle,
    has_focused: bool,
}

impl Focusable for CreationModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl CreationModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        table: OperationTables,
        columns: Vec<String>,
        cx: &mut App,
    ) -> Self {
        let values = vec![String::new(); columns.len()];
        let cursors = vec![0; columns.len()];
        Self {
            workspace,
            table,
            table_name: table.as_table_name().to_string(),
            columns,
            values,
            cursors,
            active_field_ix: 0,
            focus_handle: cx.focus_handle(),
            has_focused: false,
        }
    }

    fn active_value_and_cursor_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        if self.active_field_ix >= self.values.len() || self.active_field_ix >= self.cursors.len() {
            return None;
        }
        let value = self.values.get_mut(self.active_field_ix)?;
        let cursor = self.cursors.get_mut(self.active_field_ix)?;
        Some((value, cursor))
    }

    fn move_active_field(&mut self, delta: isize) {
        if self.columns.is_empty() {
            self.active_field_ix = 0;
            return;
        }
        let len = self.columns.len() as isize;
        let next = (self.active_field_ix as isize + delta).clamp(0, len - 1);
        self.active_field_ix = next as usize;
    }

    fn insert_char(&mut self, ch: char) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        let current = (*cursor).min(value.chars().count());
        let byte_ix = value
            .char_indices()
            .nth(current)
            .map(|(ix, _)| ix)
            .unwrap_or(value.len());
        value.insert(byte_ix, ch);
        *cursor += 1;
    }

    fn delete_backward(&mut self) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        if *cursor == 0 {
            return;
        }
        let start_char = *cursor - 1;
        let start = value
            .char_indices()
            .nth(start_char)
            .map(|(ix, _)| ix)
            .unwrap_or(0);
        let end = value
            .char_indices()
            .nth(*cursor)
            .map(|(ix, _)| ix)
            .unwrap_or(value.len());
        value.replace_range(start..end, "");
        *cursor = start_char;
    }

    fn delete_forward(&mut self) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        let len = value.chars().count();
        if *cursor >= len {
            return;
        }
        let start = value
            .char_indices()
            .nth(*cursor)
            .map(|(ix, _)| ix)
            .unwrap_or(value.len());
        let end = value
            .char_indices()
            .nth(*cursor + 1)
            .map(|(ix, _)| ix)
            .unwrap_or(value.len());
        value.replace_range(start..end, "");
    }

    fn move_left(&mut self) {
        let Some((_, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        *cursor = (*cursor).saturating_sub(1);
    }

    fn move_right(&mut self) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        *cursor = (*cursor + 1).min(value.chars().count());
    }

    fn move_home(&mut self) {
        let Some((_, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        *cursor = 0;
    }

    fn move_end(&mut self) {
        let Some((value, cursor)) = self.active_value_and_cursor_mut() else {
            return;
        };
        *cursor = value.chars().count();
    }

    fn display_field_value(&self, field_ix: usize) -> String {
        let Some(value) = self.values.get(field_ix) else {
            return String::new();
        };
        if field_ix != self.active_field_ix {
            return value.clone();
        }
        let cursor = self
            .cursors
            .get(field_ix)
            .copied()
            .unwrap_or(0)
            .min(value.chars().count());
        let before = value.chars().take(cursor).collect::<String>();
        let after = value.chars().skip(cursor).collect::<String>();
        format!("{before}|{after}")
    }

    fn close_modal(&self, cx: &mut Context<Self>) {
        let weak = self.workspace.clone();
        let _ = weak.update(cx, |ws, cx| {
            ws.close_creation_modal(cx);
        });
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let mut values = HashMap::new();
        for (column, value) in self.columns.iter().zip(self.values.iter()) {
            let trimmed = value.trim().to_string();
            if !trimmed.is_empty() {
                values.insert(column.clone(), trimmed);
            }
        }
        self.values.iter_mut().for_each(String::clear);
        self.cursors.iter_mut().for_each(|cursor| *cursor = 0);
        self.active_field_ix = 0;
        let table = self.table;
        let weak = self.workspace.clone();
        let _ = weak.update(cx, |ws, cx| {
            ws.create_row_from_modal(table, values, cx);
        });
    }
}

impl Render for CreationModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.has_focused {
            cx.focus_self(window);
            self.has_focused = true;
        }

        let title = format!("Create in {}", self.table_name);

        div()
            .id("creation_modal")
            .w(px(480.0))
            .max_h(px(620.0))
            .bg(mantle())
            .border_2()
            .border_color(green())
            .rounded_lg()
            .shadow_lg()
            .overflow_hidden()
            .focusable()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let modifiers = event.keystroke.modifiers;
                let with_save_mod = modifiers.control || modifiers.alt;

                if key == "escape" {
                    this.close_modal(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }

                if (with_save_mod && key == "s") || (key == "enter" && modifiers.control) {
                    this.submit(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }

                match key {
                    "tab" => {
                        if modifiers.control || modifiers.shift {
                            this.move_active_field(-1);
                        } else {
                            this.move_active_field(1);
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "down" | "arrowdown" => {
                        this.move_active_field(1);
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "up" | "arrowup" => {
                        this.move_active_field(-1);
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "left" | "arrowleft" => {
                        this.move_left();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "right" | "arrowright" => {
                        this.move_right();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "home" => {
                        this.move_home();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "end" => {
                        this.move_end();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "backspace" => {
                        this.delete_backward();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "delete" => {
                        this.delete_forward();
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    "enter" => {
                        let last_field_ix = this.columns.len().saturating_sub(1);
                        if this.active_field_ix >= last_field_ix {
                            this.submit(cx);
                        } else {
                            this.move_active_field(1);
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    _ => {
                        if let Some(key_char) = &event.keystroke.key_char
                            && let Some(ch) = key_char.chars().next()
                        {
                            if ch == '\t' {
                                if modifiers.control || modifiers.shift {
                                    this.move_active_field(-1);
                                } else {
                                    this.move_active_field(1);
                                }
                                window.prevent_default();
                                cx.stop_propagation();
                            } else if !modifiers.control && !modifiers.alt && !modifiers.platform {
                                this.insert_char(ch);
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        }
                    }
                }

                cx.notify();
            }))
            .child(
                div()
                    .bg(green())
                    .text_color(crust())
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .child(div().text_sm().font_weight(FontWeight::BOLD).child(title))
                    .child(
                        div()
                            .bg(red())
                            .hover(|s| s.bg(yellow()))
                            .text_color(crust())
                            .rounded_sm()
                            .px_2()
                            .py_1()
                            .child("Close")
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.close_modal(cx);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .max_h(px(500.0))
                    .overflow_y_scrollbar()
                    .children(
                        self.columns
                            .iter()
                            .enumerate()
                            .map(|(ix, column)| {
                                let field_ix = ix;
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().text_xs().text_color(text()).child(column.clone()))
                                    .child(
                                        div()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(if self.active_field_ix == ix {
                                                blue()
                                            } else {
                                                surface1()
                                            })
                                            .bg(surface0())
                                            .px_2()
                                            .py_1()
                                            .text_color(text())
                                            .child(self.display_field_value(ix))
                                            .on_mouse_up(
                                                MouseButton::Left,
                                                cx.listener(move |this, _event, window, cx| {
                                                    this.active_field_ix = field_ix;
                                                    cx.focus_self(window);
                                                    cx.notify();
                                                }),
                                            ),
                                    )
                            })
                            .collect::<Vec<_>>(),
                    ),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(surface1())
                    .p_3()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_xs().text_color(text()).child("Ctrl+S to submit"))
                    .child(
                        div()
                            .bg(green())
                            .hover(|s| s.bg(yellow()))
                            .text_color(crust())
                            .rounded_sm()
                            .px_3()
                            .py_1()
                            .child("Create")
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.submit(cx);
                                }),
                            ),
                    ),
            )
    }
}
