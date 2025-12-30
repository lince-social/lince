use crate::{
    infrastructure::database::repositories::collection::CollectionRow,
    presentation::gpui::{themes::catppuccin_macchiato::*, workspace::Workspace},
};
use gpui::*;

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

impl Render for CollectionList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = self.workspace.clone();

        div().p_2().children(
            self.collections
                .iter()
                .map(|(collection, views)| {
                    let collection_id = collection.id;
                    let weak = weak.clone();
                    div()
                        .p_2()
                        .flex()
                        .flex_row()
                        .child(
                            div()
                                .m_1()
                                .p_1()
                                .rounded_sm()
                                .bg(surface0())
                                .text_color(text())
                                .hover(|s| s.bg(surface1()))
                                .child(collection.name.clone())
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(move |_this, _evt, _win, cx| {
                                        if let Some(workspace) = weak.upgrade() {
                                            workspace.update(cx, |ws, cx| {
                                                ws.on_collection_selected(collection_id, cx);
                                            });
                                        }
                                    }),
                                ),
                        )
                        .children(
                            views
                                .iter()
                                .map(|view| div().child(view.name.clone()))
                                .collect::<Vec<_>>(),
                        )
                })
                .collect::<Vec<_>>(),
        )
    }
}
