use crate::{
    app::{Backspace, Enter},
    themes::catppuccin_macchiato::base,
};

use super::super::workspace::Workspace;
use gpui::{Context, IntoElement, Render, Window, div, *};

#[derive(Clone)]
pub struct Operation {
    pub hovered: bool,
    pub input_text: String,
    pub workspace: WeakEntity<Workspace>,
    pub placeholder: SharedString,
    pub focus_handle: FocusHandle,
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
            input_text: "".to_string(),
            workspace,
            placeholder: SharedString::from(&"Type your operation...".to_string()),
            focus_handle,
        }
    }
}

impl Render for Operation {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = self.workspace.clone();
        cx.focus_self(window);

        div()
            .id("operation")
            .bg(base())
            .p_2()
            .rounded_xs()
            .track_focus(&self.focus_handle(cx))
            .flex()
            .gap_1()
            .flex_col()
            .on_hover(cx.listener(|this, hovered, _window, _cx| {
                this.hovered = *hovered;
            }))
            .focusable()
            .child(self.input_text.clone())
            .on_action(cx.listener(|this, _event: &Backspace, _window, cx| {
                this.input_text.pop();
                cx.notify();
            }))
            .on_action(cx.listener(move |this, _event: &Enter, _window, cx| {
                if let Err(e) = weak.update(cx, |ws, cx| {
                    ws.send_operation(cx, this.input_text.clone());
                }) {
                    println!("Error in changing operation in gpui: {e}");
                } else {
                    println!("Sent operation successfully");
                }
                this.input_text = String::new();
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if let Some(key_char) = &event.keystroke.key_char {
                    if key_char.len() == 1 && !event.keystroke.modifiers.control {
                        this.input_text.push_str(key_char);
                        cx.notify();
                    }
                }
            }))
    }
}
