use crate::themes::catppuccin_macchiato::{blue, red};

use super::super::{themes::catppuccin_macchiato::*, workspace::Workspace};
use domain::dirty::collection::CollectionRow;
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div, *,
};

#[derive(Clone)]
pub struct CollectionList {
    pub hovered: bool,
    pub hovered_collection_id: Option<u32>,
    pub collections: Vec<CollectionRow>,
    pub workspace: WeakEntity<Workspace>,
}

impl CollectionList {
    pub fn new(collections: Vec<CollectionRow>, workspace: WeakEntity<Workspace>) -> Self {
        Self {
            hovered: false,
            hovered_collection_id: None,
            collections,
            workspace,
        }
    }
}

#[derive(Clone, IntoElement)]
struct CollectionButton {
    id: u32,
    name: SharedString,
    show_add: bool,
    workspace: WeakEntity<Workspace>,
}

impl RenderOnce for CollectionButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let id = self.id;
        let weak = self.workspace.clone();
        let weak_for_add = self.workspace;

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
            .children(if self.show_add {
                Some(
                    div()
                        .bg(surface0())
                        .hover(|s| s.bg(surface1()))
                        .rounded_xs()
                        .p_0()
                        .px_1()
                        .child("+")
                        .on_mouse_up(MouseButton::Left, move |_evt, _win, cx| {
                            if let Some(ws) = weak_for_add.upgrade() {
                                ws.update(cx, |ws, cx| {
                                    ws.open_collection_view_creation_modal(id, cx);
                                    cx.stop_propagation();
                                });
                            }
                        })
                        .into_any_element(),
                )
            } else {
                None
            })
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
    name: SharedString,
    collection_id: u32,
    workspace: WeakEntity<Workspace>,
}

impl RenderOnce for CollectionViewRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let workspace_for_toggle = self.workspace.clone();

        div()
            .p_0()
            .px_1()
            .flex()
            .flex_row()
            .gap_1()
            .items_center()
            .child(
                div()
                    .rounded_xs()
                    .text_color(crust())
                    .bg(if self.quantity == 0 { red() } else { blue() })
                    .items_center()
                    .hover(|s| s.bg(if self.quantity == 0 { peach() } else { mauve() }))
                    .child(self.name)
                    .on_mouse_up(MouseButton::Left, move |_evt, _win, cx| {
                        if let Some(ws) = workspace_for_toggle.upgrade() {
                            ws.update(cx, |ws, cx| {
                                ws.on_view_selected(cx, self.collection_id, self.id);
                            });
                        }
                    }),
            )
    }
}
impl Render for CollectionList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = self.workspace.clone();

        div()
            .id("hoverable_collection")
            .bg(base())
            .p_2()
            .rounded_xs()
            .flex()
            .gap_1()
            .flex_col()
            .on_hover(cx.listener(|this, hovered, _window, _cx| {
                this.hovered = *hovered;
            }))
            .children(
                self.collections[..if self.hovered {
                    self.collections.len()
                } else {
                    self.collections.len().min(1)
                }]
                    .iter()
                    .map(|(collection, views)| {
                        let collection_id = collection.id;
                        div()
                            .id(("collection_row", collection_id))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .on_hover(cx.listener(move |this, hovered, _window, cx| {
                                if *hovered {
                                    this.hovered_collection_id = Some(collection_id);
                                } else if this.hovered_collection_id == Some(collection_id) {
                                    this.hovered_collection_id = None;
                                }
                                cx.notify();
                            }))
                            .child(CollectionButton {
                                id: collection_id,
                                name: SharedString::from(&collection.name),
                                show_add: self.hovered_collection_id == Some(collection_id),
                                workspace: weak.clone(),
                            })
                            .children(
                                views
                                    .iter()
                                    .map(|view| CollectionViewRow {
                                        id: view.id,
                                        quantity: view.quantity,
                                        name: SharedString::from(&view.name),
                                        collection_id,
                                        workspace: weak.clone(),
                                    })
                                    .collect::<Vec<_>>(),
                            )
                    })
                    .collect::<Vec<_>>(),
            )
    }
}
