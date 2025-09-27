use gpui::{
    App, AppContext, Application, Bounds, Context, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseUpEvent, ParentElement, Render, Styled, Window,
    WindowBounds, WindowOptions, actions, div, prelude::FluentBuilder, px, rgb, size,
};

use crate::{
    application::providers::collection::CollectionRow, domain::entities::collection::Collection,
};

actions!(todo, [AddTodo, Backspace, ClearInput]);

pub fn gpui_app() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            gpui::KeyBinding::new("enter", AddTodo, None),
            gpui::KeyBinding::new("backspace", Backspace, None),
            gpui::KeyBinding::new("escape", ClearInput, None),
        ]);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Maximized(Bounds::centered(
                    None,
                    size(px(1920.), px(1080.)),
                    cx,
                ))),
                ..Default::default()
            },
            |_, cx| cx.new(LinceApp::new),
        )
        .unwrap()
        .update(cx, |view, window, cx| {
            window.focus(&view.focus_handle(cx));
            cx.activate(true);
        })
        .unwrap();
    });
}

#[derive(Clone, Debug)]
struct Todo {
    id: usize,
    text: String,
}

impl Todo {
    fn new(id: usize, text: String) -> Self {
        Self { id, text }
    }
}
pub struct LinceData {
    pub collection: Vec<CollectionRow>,
}

impl LinceData {
    fn default() -> Self {
        Self {
            collection: Vec::new(),
        }
    }
}

struct LinceApp {
    focus_handle: FocusHandle,
    data: LinceData,
    todos: Vec<Todo>,
    input_text: String,
    next_id: usize,
}

impl LinceApp {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            data: LinceData::default(),
            todos: Vec::new(),
            input_text: String::new(),
            next_id: 1,
        }
    }

    fn add_todo(&mut self, _: &AddTodo, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_text.trim().is_empty() {
            let todo = Todo::new(self.next_id, self.input_text.trim().to_string());
            self.todos.push(todo);
            self.next_id += 1;
            self.input_text.clear();
            cx.notify();
        }
    }
    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_text.is_empty() {
            self.input_text.pop();
            cx.notify();
        }
    }
    fn clear_input(&mut self, _: &ClearInput, _: &mut Window, cx: &mut Context<Self>) {
        self.input_text.clear();
        cx.notify();
    }

    fn on_add_click(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_text.trim().is_empty() {
            let todo = Todo::new(self.next_id, self.input_text.trim().to_string());
            self.todos.push(todo);
            self.next_id += 1;
            self.input_text.clear();
            cx.notify();
        }
    }
    fn delete_todo(&mut self, todo_id: usize, cx: &mut Context<Self>) {
        self.todos.retain(|todo| todo.id != todo_id);
        cx.notify();
    }
}

impl Focusable for LinceApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LinceApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .bg(rgb(0x1a1a1a))
            .size_full()
            .p_6()
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::add_todo))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::clear_input))
            // Handle regular character input
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if let Some(key_char) = &event.keystroke.key_char
                    && key_char.len() == 1
                    && !event.keystroke.modifiers.control
                {
                    this.input_text.push_str(key_char);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .text_3xl()
                    .text_color(rgb(0xe0e0e0))
                    .mb_4()
                    .child("📝 Simple Todo App"),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .bg(rgb(0x2a2a2a))
                            .border_1()
                            .border_color(rgb(0x444444))
                            .rounded_lg()
                            .px_4()
                            .py_3()
                            .text_color(rgb(0xe0e0e0))
                            .child(if self.input_text.is_empty() {
                                div().text_color(rgb(0x888888)).child("Type a new todo...")
                            } else {
                                div().child(self.input_text.clone())
                            }),
                    )
                    .child(
                        div()
                            .bg(rgb(0x4a9eff))
                            .hover(|style| style.bg(rgb(0x357abd)).cursor_pointer())
                            .border_1()
                            .border_color(rgb(0x357abd))
                            .rounded_lg()
                            .px_6()
                            .py_3()
                            .text_color(rgb(0xffffff))
                            .child("Add")
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_add_click)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(self.todos.iter().map(|todo| {
                        let todo_id = todo.id;

                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .bg(rgb(0x2a2a2a))
                            .border_1()
                            .border_color(rgb(0x444444))
                            .rounded_lg()
                            .p_4()
                            .hover(|style| style.bg(rgb(0x333333)))
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(rgb(0xe0e0e0))
                                    .child(todo.text.clone()),
                            )
                            .child(
                                div()
                                    .bg(rgb(0x7c4a4a))
                                    .hover(|style| style.bg(rgb(0x9c5a5a)).cursor_pointer())
                                    .border_1()
                                    .border_color(rgb(0x944444))
                                    .rounded_lg()
                                    .px_3()
                                    .py_2()
                                    .text_color(rgb(0xffffff))
                                    .text_xs()
                                    .child("Delete")
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(move |this, _e, _w, cx| {
                                            this.delete_todo(todo_id, cx);
                                        }),
                                    ),
                            )
                    }))
                    .when(self.todos.is_empty(), |container| {
                        container.child(
                            div()
                                .text_center()
                                .text_color(rgb(0x888888))
                                .py_8()
                                .child("No todos yet. Start typing and press Enter!"),
                        )
                    }),
            )
    }
}
