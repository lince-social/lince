use crate::{
    infrastructure::database::repositories::collection::CollectionRow,
    presentation::gpui::{
        themes::catppuccin_mocha::{self, red, *},
        workspace::Workspace,
    },
};
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, *,
};

#[derive(Clone)]
pub struct CollectionList {
    pub collections: Vec<CollectionRow>,
    pub workspace: WeakEntity<Workspace>,
}

impl CollectionList {
    pub fn new(collections: Vec<CollectionRow>, workspace: WeakEntity<Workspace>) -> Self {
        Self {
            collections,
            workspace,
        }
    }
}

#[derive(Clone, IntoElement)]
struct CollectionButton {
    id: u32,
    name: String,
    workspace: WeakEntity<Workspace>,
}

impl RenderOnce for CollectionButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let id = self.id;
        let weak = self.workspace;

        div()
            .p_0()
            .px_1()
            .items_center()
            .flex()
            .flex_row()
            .gap_1()
            .child(
                div()
                    .bg(surface0())
                    .hover(|s| s.bg(surface1()))
                    .rounded_xs()
                    .p_0()
                    .px_1()
                    .child(self.id.to_string()),
            )
            .rounded_xs()
            .text_color(text())
            .child(
                div()
                    .hover(|s| s.bg(surface1()))
                    .bg(surface0())
                    .rounded_xs()
                    .child(self.name),
            )
            .on_mouse_up(MouseButton::Left, move |_evt, _win, cx| {
                if let Some(ws) = weak.upgrade() {
                    ws.update(cx, |ws, cx| {
                        ws.on_collection_selected(id, cx);
                    });
                }
            })
    }
}

#[derive(Clone, IntoElement)]
struct CollectionViewRow {
    id: u32,
    quantity: i32,
    name: String,

    collection_id: u32,

    workspace: WeakEntity<Workspace>,
}

impl RenderOnce for CollectionViewRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .p_0()
            .px_1()
            .rounded_xs()
            .text_color(crust())
            .bg(if self.quantity == 0 {
                red()
            } else {
                catppuccin_mocha::blue()
            })
            .items_center()
            .hover(|s| s.bg(if self.quantity == 0 { peach() } else { mauve() }))
            .child(self.name)
            .on_mouse_up(MouseButton::Left, move |_evt, _win, cx| {
                if let Some(ws) = self.workspace.upgrade() {
                    ws.update(cx, |ws, cx| {
                        ws.on_view_selected(cx, self.collection_id, self.id);
                    });
                }
            })
    }
}

impl Render for CollectionList {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let weak = self.workspace.clone();

        div()
            .id("hoverable_collection")
            .bg(base())
            .p_2()
            .rounded_xs()
            .flex()
            .gap_1()
            .flex_col()
            .children(
                self.collections
                    .iter()
                    .map(|(collection, views)| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .child(CollectionButton {
                                id: collection.id,
                                name: collection.name.clone(),
                                workspace: weak.clone(),
                            })
                            .gap_1()
                            .children(
                                views
                                    .iter()
                                    .map(|view| CollectionViewRow {
                                        id: view.id,
                                        quantity: view.quantity,
                                        name: view.name.clone(),
                                        collection_id: collection.id,
                                        workspace: weak.clone(),
                                    })
                                    .collect::<Vec<_>>(),
                            )
                    })
                    .collect::<Vec<_>>(),
            )
            .on_hover(|hovered, _window, _app| {
                println!(
                    "{}",
                    if *hovered {
                        "Hover started"
                    } else {
                        "Hover ended"
                    }
                );
            })
    }
}
