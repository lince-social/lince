use application::command::{
    CommandOrigin, CommandSessionSnapshot, CommandSessionStatus, command_buffer_snapshot,
    command_buffer_subscribe,
};
use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, MouseUpEvent, ParentElement, Render,
    Styled, Window, div, px,
};
use gpui_component::scroll::ScrollableElement;
use injection::cross_cutting::InjectedServices;
use std::{collections::HashMap, time::Duration};

use crate::themes::catppuccin_mocha::{
    crust, green, mantle, overlay0, red, surface0, surface1, text, yellow,
};

pub struct CommandNotifications {
    services: InjectedServices,
    sessions: Vec<CommandSessionSnapshot>,
    collapsed: HashMap<u64, bool>,
    dismissed: HashMap<u64, bool>,
    show_notifications: bool,
    notification_seconds: f64,
}

impl CommandNotifications {
    pub fn new(services: InjectedServices, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            services,
            sessions: Vec::new(),
            collapsed: HashMap::new(),
            dismissed: HashMap::new(),
            show_notifications: false,
            notification_seconds: -1.0,
        };
        this.start_session_listener(cx);
        this.start_configuration_poller(cx);
        this
    }

    fn start_session_listener(&mut self, cx: &mut Context<Self>) {
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

    fn start_configuration_poller(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            loop {
                let config = services.repository.configuration.get_active().await.ok();
                if this
                    .update(cx, |this, cx| {
                        if let Some(config) = config {
                            this.show_notifications = config.show_command_notifications > 0;
                            this.notification_seconds = config.command_notification_seconds;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .detach();
    }

    fn sync_sessions(&mut self, sessions: Vec<CommandSessionSnapshot>) {
        for session in &sessions {
            self.collapsed.entry(session.session_id).or_insert(false);
            self.dismissed.entry(session.session_id).or_insert(false);
        }
        self.sessions = sessions;
    }

    fn origin_label(origin: &CommandOrigin) -> String {
        match origin {
            CommandOrigin::Consequence(karma_id) => format!("consequence k{karma_id}"),
            CommandOrigin::Operation => "operation".to_string(),
        }
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

    fn elapsed_seconds(session: &CommandSessionSnapshot) -> f64 {
        match session.finished_at {
            Some(end) => end.duration_since(session.started_at).as_secs_f64(),
            None => session.started_at.elapsed().as_secs_f64(),
        }
    }

    fn should_show_session(&self, session: &CommandSessionSnapshot) -> bool {
        if !self.show_notifications || self.notification_seconds < 0.0 {
            return false;
        }
        if self
            .dismissed
            .get(&session.session_id)
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        Self::elapsed_seconds(session) >= self.notification_seconds
    }

    fn preview(output: &str) -> String {
        let line = output
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("-");
        let mut preview = line.chars().take(90).collect::<String>();
        if line.chars().count() > 90 {
            preview.push_str("...");
        }
        preview
    }
}

impl Render for CommandNotifications {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cards = self
            .sessions
            .iter()
            .filter(|session| self.should_show_session(session))
            .filter_map(|session| {
                let session_id = session.session_id;
                let collapsed = self.collapsed.get(&session_id).copied().unwrap_or(false);
                let origin = Self::origin_label(&session.origin);
                let status = Self::status_label(&session.status);
                let preview = Self::preview(&session.output);
                let command_id = session
                    .command_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let is_running = matches!(session.status, CommandSessionStatus::Running);
                Some(
                    div()
                        .w(px(430.0))
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
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_2()
                                        .child(if collapsed { ">" } else { "v" })
                                        .child(format!(
                                            "Command {command_id} | session {session_id} | {origin}"
                                        ))
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(
                                                move |this, _event: &MouseUpEvent, _window, cx| {
                                                    let entry =
                                                        this.collapsed.entry(session_id).or_insert(false);
                                                    *entry = !*entry;
                                                    cx.notify();
                                                },
                                            ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_1()
                                        .items_center()
                                        .child(
                                            div()
                                                .px_2()
                                                .rounded_sm()
                                                .bg(if is_running { green() } else { red() })
                                                .text_color(crust())
                                                .child(status),
                                        )
                                        .child(
                                            div()
                                                .px_2()
                                                .rounded_sm()
                                                .bg(surface1())
                                                .text_color(text())
                                                .child("x")
                                                .on_mouse_up(
                                                    MouseButton::Left,
                                                    cx.listener(
                                                        move |this,
                                                              _event: &MouseUpEvent,
                                                              _window,
                                                              cx| {
                                                            this.dismissed.insert(session_id, true);
                                                            cx.notify();
                                                        },
                                                    ),
                                                ),
                                        ),
                                ),
                        )
                        .child(if collapsed {
                            div()
                                .p_2()
                                .bg(surface0())
                                .text_color(text())
                                .child(preview)
                                .into_any_element()
                        } else {
                            div()
                                .bg(surface0())
                                .text_color(text())
                                .border_t_1()
                                .border_color(overlay0())
                                .p_2()
                                .max_h(px(240.0))
                                .overflow_y_scrollbar()
                                .child(session.output.clone())
                                .into_any_element()
                        }),
                )
            })
            .collect::<Vec<_>>();

        if cards.is_empty() {
            return div().into_any_element();
        }

        div()
            .absolute()
            .right(px(14.0))
            .top(px(54.0))
            .flex()
            .flex_col()
            .gap_2()
            .children(cards)
            .into_any_element()
    }
}
