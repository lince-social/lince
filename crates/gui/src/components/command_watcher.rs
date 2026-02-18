use application::command::{
    CommandOrigin, CommandSessionSnapshot, CommandSessionStatus, command_buffer_send_input,
    command_buffer_snapshot, command_buffer_subscribe,
};
use gpui::{
    App, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::scroll::ScrollableElement;
use ropey::Rope;
use std::collections::HashMap;

use crate::themes::catppuccin_mocha::{
    base, crust, green, mantle, overlay0, red, surface0, surface1, text, yellow,
};

#[derive(Clone)]
struct SessionUiState {
    collapsed: bool,
    width: f32,
    height: f32,
    input: Rope,
    cursor_pos: usize,
}

impl SessionUiState {
    fn new() -> Self {
        Self {
            collapsed: false,
            width: 820.0,
            height: 360.0,
            input: Rope::new(),
            cursor_pos: 0,
        }
    }

    fn text_len(&self) -> usize {
        self.input.len_chars()
    }
}

#[derive(Clone, Copy)]
struct SessionResizeState {
    session_id: u64,
    start_x: f32,
    start_y: f32,
    start_width: f32,
    start_height: f32,
}

pub struct CommandWatcher {
    sessions: Vec<CommandSessionSnapshot>,
    ui_by_session: HashMap<u64, SessionUiState>,
    active_session: Option<u64>,
    resize_state: Option<SessionResizeState>,
    focus_handle: FocusHandle,
}

impl Focusable for CommandWatcher {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl CommandWatcher {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            sessions: Vec::new(),
            ui_by_session: HashMap::new(),
            active_session: None,
            resize_state: None,
            focus_handle: cx.focus_handle(),
        };
        this.start_listener(cx);
        this
    }

    fn start_listener(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let initial_sessions = command_buffer_snapshot().await;
            if this
                .update(cx, |this, cx| {
                    this.sync_sessions(initial_sessions);
                    cx.notify();
                })
                .is_err()
            {
                return;
            }

            let mut subscriber = command_buffer_subscribe();
            while subscriber.recv().await.is_some() {
                let snapshot = command_buffer_snapshot().await;
                if this
                    .update(cx, |this, cx| {
                        this.sync_sessions(snapshot);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn sync_sessions(&mut self, sessions: Vec<CommandSessionSnapshot>) {
        for session in &sessions {
            self.ui_by_session
                .entry(session.session_id)
                .or_insert_with(SessionUiState::new);
        }
        if let Some(active) = self.active_session
            && !sessions.iter().any(|session| session.session_id == active)
        {
            self.active_session = None;
        }
        self.sessions = sessions;
    }

    fn is_running(status: &CommandSessionStatus) -> bool {
        matches!(status, CommandSessionStatus::Running)
    }

    fn status_label(status: &CommandSessionStatus) -> String {
        match status {
            CommandSessionStatus::Running => "running".to_string(),
            CommandSessionStatus::Finished(code) => match code {
                Some(code) => format!("finished ({code})"),
                None => "finished".to_string(),
            },
            CommandSessionStatus::Failed(message) => format!("failed ({message})"),
        }
    }

    fn origin_label(origin: &CommandOrigin) -> String {
        match origin {
            CommandOrigin::Consequence(karma_id) => format!("consequence k{karma_id}"),
            CommandOrigin::Operation => "operation".to_string(),
        }
    }

    fn begin_resize(
        &mut self,
        session_id: u64,
        event: &MouseDownEvent,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) {
        self.resize_state = Some(SessionResizeState {
            session_id,
            start_x: f32::from(event.position.x),
            start_y: f32::from(event.position.y),
            start_width: width,
            start_height: height,
        });
        cx.notify();
    }

    fn update_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !event.dragging() {
            return;
        }
        let Some(resize_state) = self.resize_state else {
            return;
        };
        let delta_x = f32::from(event.position.x) - resize_state.start_x;
        let delta_y = f32::from(event.position.y) - resize_state.start_y;
        if let Some(ui_state) = self.ui_by_session.get_mut(&resize_state.session_id) {
            ui_state.width = (resize_state.start_width + delta_x).clamp(420.0, 1800.0);
            ui_state.height = (resize_state.start_height + delta_y).clamp(220.0, 1200.0);
            cx.notify();
        }
    }

    fn end_resize(&mut self, cx: &mut Context<Self>) {
        if self.resize_state.is_some() {
            self.resize_state = None;
            cx.notify();
        }
    }

    fn active_ui_mut(&mut self) -> Option<(&mut SessionUiState, u64)> {
        let session_id = self.active_session?;
        let session_ui = self.ui_by_session.get_mut(&session_id)?;
        Some((session_ui, session_id))
    }

    fn send_active_input(&mut self, cx: &mut Context<Self>) {
        let Some((session_ui, session_id)) = self.active_ui_mut() else {
            return;
        };
        let mut text = session_ui.input.to_string();
        text.push('\n');
        session_ui.input = Rope::new();
        session_ui.cursor_pos = 0;
        cx.spawn(async move |_this, _cx| {
            let _ = command_buffer_send_input(session_id, text).await;
        })
        .detach();
        cx.notify();
    }

    fn insert_active_char(&mut self, ch: char, cx: &mut Context<Self>) {
        let Some((session_ui, _)) = self.active_ui_mut() else {
            return;
        };
        let pos = session_ui.cursor_pos.min(session_ui.text_len());
        session_ui.input.insert_char(pos, ch);
        session_ui.cursor_pos = pos + 1;
        cx.notify();
    }

    fn delete_active_char(&mut self, cx: &mut Context<Self>) {
        let Some((session_ui, _)) = self.active_ui_mut() else {
            return;
        };
        if session_ui.cursor_pos == 0 {
            return;
        }
        let new_pos = session_ui.cursor_pos - 1;
        session_ui.input.remove(new_pos..session_ui.cursor_pos);
        session_ui.cursor_pos = new_pos;
        cx.notify();
    }

    fn move_cursor_left(&mut self, cx: &mut Context<Self>) {
        let Some((session_ui, _)) = self.active_ui_mut() else {
            return;
        };
        session_ui.cursor_pos = session_ui.cursor_pos.saturating_sub(1);
        cx.notify();
    }

    fn move_cursor_right(&mut self, cx: &mut Context<Self>) {
        let Some((session_ui, _)) = self.active_ui_mut() else {
            return;
        };
        let len = session_ui.text_len();
        session_ui.cursor_pos = (session_ui.cursor_pos + 1).min(len);
        cx.notify();
    }

    fn move_cursor_home(&mut self, cx: &mut Context<Self>) {
        let Some((session_ui, _)) = self.active_ui_mut() else {
            return;
        };
        session_ui.cursor_pos = 0;
        cx.notify();
    }

    fn move_cursor_end(&mut self, cx: &mut Context<Self>) {
        let Some((session_ui, _)) = self.active_ui_mut() else {
            return;
        };
        session_ui.cursor_pos = session_ui.text_len();
        cx.notify();
    }

    fn toggle_session(&mut self, session_id: u64, cx: &mut Context<Self>) {
        if let Some(session_ui) = self.ui_by_session.get_mut(&session_id) {
            session_ui.collapsed = !session_ui.collapsed;
            cx.notify();
        }
    }

    fn input_line_text(&self, session_id: u64, session_ui: &SessionUiState) -> String {
        let text = session_ui.input.to_string();
        let chars = text.chars().collect::<Vec<_>>();
        let cursor = session_ui.cursor_pos.min(chars.len());
        let before = chars.iter().take(cursor).collect::<String>();
        let after = chars.iter().skip(cursor).collect::<String>();
        if self.active_session == Some(session_id) {
            format!("{before}|{after}")
        } else {
            format!("{before}{after}")
        }
    }

    fn output_preview(output: &str) -> String {
        let line = output
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        let mut result = line.chars().take(120).collect::<String>();
        if line.chars().count() > 120 {
            result.push_str("...");
        }
        result
    }
}

impl Render for CommandWatcher {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session_cards = self
            .sessions
            .iter()
            .filter_map(|session| {
                let session_ui = self.ui_by_session.get(&session.session_id)?.clone();
                let is_running = Self::is_running(&session.status);
                let status = Self::status_label(&session.status);
                let origin = Self::origin_label(&session.origin);
                let command_id = session
                    .command_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let is_active_input = self.active_session == Some(session.session_id);
                let input_text = self.input_line_text(session.session_id, &session_ui);
                let output_preview = Self::output_preview(&session.output);
                let output_height = (session_ui.height - 96.0).max(100.0);
                let resize_width = session_ui.width;
                let resize_height = session_ui.height;
                let session_id = session.session_id;
                Some(
                    div()
                        .relative()
                        .w(px(session_ui.width))
                        .bg(mantle())
                        .border_2()
                        .border_color(yellow())
                        .rounded_lg()
                        .shadow_lg()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .bg(yellow())
                                .text_color(crust())
                                .p_2()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event: &MouseUpEvent, _window, cx| {
                                        this.toggle_session(session_id, cx);
                                    }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_2()
                                        .child(if session_ui.collapsed { ">" } else { "v" })
                                        .child(format!(
                                            "Running Command | Session: {session_id}, Id: {command_id}, Origin: {origin}"
                                        )),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .rounded_sm()
                                        .bg(if is_running { green() } else { red() })
                                        .text_color(crust())
                                        .child(status),
                                ),
                        )
                        .child(
                            if session_ui.collapsed {
                                div()
                                    .p_2()
                                    .bg(surface0())
                                    .text_color(text())
                                    .child(output_preview)
                                    .into_any_element()
                            } else {
                                div()
                                    .flex()
                                    .flex_col()
                                    .h(px(session_ui.height))
                                    .child(
                                        div()
                                            .h(px(output_height))
                                            .bg(base())
                                            .text_color(text())
                                            .text_sm()
                                            .p_2()
                                            .overflow_y_scrollbar()
                                            .child(session.output.clone()),
                                    )
                                    .child(
                                        div()
                                            .h(px(36.0))
                                            .bg(surface0())
                                            .border_t_1()
                                            .border_color(overlay0())
                                            .text_color(text())
                                            .p_2()
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(
                                                    move |this, _event: &MouseDownEvent, window, cx| {
                                                        this.active_session = Some(session_id);
                                                        cx.focus_self(window);
                                                        cx.notify();
                                                    },
                                                ),
                                            )
                                            .child(format!(
                                                "{} {}",
                                                if is_active_input { ">" } else { "-" },
                                                input_text
                                            )),
                                    )
                                    .child(
                                        div()
                                            .h(px(12.0))
                                            .w_full()
                                            .flex()
                                            .justify_end()
                                            .bg(surface1())
                                            .child(
                                                div()
                                                    .h(px(12.0))
                                                    .w(px(12.0))
                                                    .bg(overlay0())
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(
                                                            move |this,
                                                                  event: &MouseDownEvent,
                                                                  _window,
                                                                  cx| {
                                                                this.begin_resize(
                                                                    session_id,
                                                                    event,
                                                                    resize_width,
                                                                    resize_height,
                                                                    cx,
                                                                );
                                                                cx.stop_propagation();
                                                            },
                                                        ),
                                                    ),
                                            ),
                                    )
                                    .into_any_element()
                            },
                        )
                        .child(
                            div()
                                .bg(surface1())
                                .text_color(text())
                                .p_2()
                                .child(session.command.clone()),
                        ),
                )
            })
            .collect::<Vec<_>>();

        div()
            .id("command-watcher")
            .w_full()
            .max_h(px(1200.0))
            .bg(surface0())
            .border_1()
            .border_color(yellow())
            .rounded_lg()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .track_focus(&self.focus_handle(cx))
            .focusable()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let Some(session_id) = this.active_session else {
                    return;
                };
                let is_running = this
                    .sessions
                    .iter()
                    .find(|session| session.session_id == session_id)
                    .map(|session| Self::is_running(&session.status))
                    .unwrap_or(false);
                if !is_running {
                    return;
                }

                let key = event.keystroke.key.as_str();
                let modifiers = event.keystroke.modifiers;
                if key == "enter" && !modifiers.control {
                    this.send_active_input(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if key == "backspace" {
                    this.delete_active_char(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if key == "left" || key == "arrowleft" {
                    this.move_cursor_left(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if key == "right" || key == "arrowright" {
                    this.move_cursor_right(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if key == "home" {
                    this.move_cursor_home(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if key == "end" {
                    this.move_cursor_end(cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if key == "space" {
                    this.insert_active_char(' ', cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if !modifiers.control
                    && !modifiers.alt
                    && key.chars().count() == 1
                    && let Some(ch) = key.chars().next()
                {
                    this.insert_active_char(ch, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                this.update_resize(event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.end_resize(cx);
                }),
            )
            .child(
                div()
                    .text_color(text())
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Command Buffer"),
            )
            .child(
                div()
                    .max_h(px(1100.0))
                    .overflow_y_scrollbar()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(session_cards),
            )
    }
}
