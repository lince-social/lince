use crate::components::table_vim::EditMode;

use super::super::workspace::Workspace;
use gpui::{Context, IntoElement, Render, Window, div, *};
use ropey::Rope;

#[derive(Clone)]
pub struct Operation {
    pub hovered: bool,
    pub input_text: Rope,
    pub workspace: WeakEntity<Workspace>,
    pub placeholder: SharedString,
    pub focus_handle: FocusHandle,
    pub has_focused: bool,
    pub cursor_pos: usize,
    pub edit_mode: EditMode,
}

impl Focusable for Operation {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Operation {
    pub fn new(workspace: WeakEntity<Workspace>, focus_handle: FocusHandle) -> Self {
        Self {
            hovered: false,
            input_text: Rope::new(),
            workspace,
            placeholder: SharedString::from(&"Type your operation...".to_string()),
            focus_handle,
            has_focused: false,
            cursor_pos: 0,
            edit_mode: EditMode::Insert,
        }
    }

    fn text_len(&self) -> usize {
        self.input_text.len_chars()
    }

    fn clamp_cursor(&mut self) {
        self.cursor_pos = self.cursor_pos.min(self.text_len());
    }

    fn move_left(&mut self) {
        self.clamp_cursor();
        self.cursor_pos = self.cursor_pos.saturating_sub(1);
    }

    fn move_right(&mut self) {
        self.clamp_cursor();
        self.cursor_pos = (self.cursor_pos + 1).min(self.text_len());
    }

    fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    fn move_end(&mut self) {
        self.cursor_pos = self.text_len();
    }

    fn is_word_char(ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_'
    }

    fn move_word_forward(&mut self) {
        self.clamp_cursor();
        let chars = self.input_text.to_string().chars().collect::<Vec<_>>();
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
    }

    fn move_word_backward(&mut self) {
        self.clamp_cursor();
        let chars = self.input_text.to_string().chars().collect::<Vec<_>>();
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
    }

    fn delete_backward(&mut self) {
        self.clamp_cursor();
        if self.cursor_pos > 0 {
            let start = self.cursor_pos - 1;
            self.input_text.remove(start..self.cursor_pos);
            self.cursor_pos = start;
        }
    }

    fn delete_forward(&mut self) {
        self.clamp_cursor();
        let len = self.text_len();
        if self.cursor_pos < len {
            self.input_text.remove(self.cursor_pos..self.cursor_pos + 1);
        }
    }

    fn insert_char(&mut self, ch: char) {
        self.clamp_cursor();
        self.input_text.insert_char(self.cursor_pos, ch);
        self.cursor_pos += 1;
    }

    fn send_operation_and_clear(&mut self, cx: &mut Context<Self>) {
        let text = self.input_text.to_string();
        let weak = self.workspace.clone();
        if let Err(e) = weak.update(cx, |ws, cx| {
            ws.send_operation(cx, text);
        }) {
            println!("Error in changing operation in gpui: {e}");
        } else {
            println!("Sent operation successfully");
        }
        self.input_text = Rope::new();
        self.cursor_pos = 0;
    }

    fn display_text_with_cursor(&self) -> AnyElement {
        let text = self.input_text.to_string();
        let chars = text.chars().collect::<Vec<_>>();
        let len = chars.len();
        let cursor = self.cursor_pos.min(len);
        match self.edit_mode {
            EditMode::Insert => {
                let before = chars.iter().take(cursor).collect::<String>();
                let after = chars.iter().skip(cursor).collect::<String>();
                div().child(format!("{before}|{after}")).into_any_element()
            }
            EditMode::Normal => {
                if chars.is_empty() {
                    return div()
                        .child(div().bg(rgb(0xffffff)).text_color(rgb(0x000000)).child(" "))
                        .into_any_element();
                }
                let ix = cursor.min(len.saturating_sub(1));
                div()
                    .flex()
                    .items_center()
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

    pub fn editing_mode_widget_label(&self, window: &Window) -> Option<&'static str> {
        if self.focus_handle.is_focused(window) {
            Some(match self.edit_mode {
                EditMode::Normal => "N",
                EditMode::Insert => "I",
            })
        } else {
            None
        }
    }
}

impl Render for Operation {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.has_focused {
            cx.focus_self(window);
            self.has_focused = true;
        }

        div()
            .id("operation")
            .h_full()
            .w_full()
            .flex()
            .items_center()
            .track_focus(&self.focus_handle(cx))
            .on_hover(cx.listener(|this, hovered, _window, _cx| {
                this.hovered = *hovered;
            }))
            .focusable()
            .gap_1()
            .child(":")
            .child(self.display_text_with_cursor())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let is_ctrl_or_alt =
                    event.keystroke.modifiers.control || event.keystroke.modifiers.alt;

                if (key == "enter" && !event.keystroke.modifiers.control)
                    || (is_ctrl_or_alt && key == "s")
                {
                    this.send_operation_and_clear(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }

                match this.edit_mode {
                    EditMode::Insert => match key {
                        "escape" => {
                            this.edit_mode = EditMode::Normal;
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "backspace" => {
                            this.delete_backward();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "delete" => {
                            this.delete_forward();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "left" | "arrowleft" => {
                            this.move_left();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "right" | "arrowright" => {
                            this.move_right();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "home" => {
                            this.move_home();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "end" => {
                            this.move_end();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        _ => {}
                    },
                    EditMode::Normal => match key {
                        "escape" => {
                            window.prevent_default();
                            cx.stop_propagation();
                            return;
                        }
                        "i" => {
                            this.edit_mode = EditMode::Insert;
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "a" => {
                            this.move_right();
                            this.edit_mode = EditMode::Insert;
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "h" | "left" | "arrowleft" => {
                            this.move_left();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "l" | "right" | "arrowright" => {
                            this.move_right();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "b" | "B" => {
                            this.move_word_backward();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "w" | "W" => {
                            this.move_word_forward();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "0" | "home" => {
                            this.move_home();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "$" | "end" => {
                            this.move_end();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        "x" => {
                            this.delete_forward();
                            window.prevent_default();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        _ => {}
                    },
                }

                if let EditMode::Insert = this.edit_mode {
                    if let Some(key_char) = &event.keystroke.key_char {
                        if key_char.chars().count() == 1
                            && !event.keystroke.modifiers.control
                            && !event.keystroke.modifiers.platform
                        {
                            if let Some(ch) = key_char.chars().next() {
                                this.insert_char(ch);
                                window.prevent_default();
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }
                    }
                }
            }))
    }
}
