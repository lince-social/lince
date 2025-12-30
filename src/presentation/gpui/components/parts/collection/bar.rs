use crate::{
    infrastructure::database::repositories::collection::CollectionRow,
    presentation::gpui::state::StateModel,
};
use gpui::*;

#[derive(Clone)]
pub struct CollectionList {
    pub collections: Vec<CollectionRow>,
}

impl CollectionList {
    pub fn new(collections: Vec<CollectionRow>) -> Self {
        Self { collections }
    }
    pub fn view(cx: &mut App, collections: Vec<CollectionRow>) -> Entity<Self> {
        cx.new(|_| Self::new(collections))
    }
}

impl Global for CollectionList {}

pub struct CollectionSelectedEvent {
    pub id: u32,
}

impl EventEmitter<CollectionSelectedEvent> for CollectionList {}

impl Render for CollectionList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().p_2().children(
            self.collections
                .iter()
                .map(|(collection, views)| {
                    div()
                        .p_2()
                        .flex()
                        .flex_row()
                        .child(collection.name.clone())
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            cx.update_global::<StateModel, _>(|model, cx| {
                                cx.emit(CollectionSelectedEvent { id: collection.id });
                            });
                        })
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

// impl RenderOnce for CollectionList {
//     fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
//         let weak = self.weak.clone();

//         div().children(
//             self.collections
//                 .into_iter()
//                 .map(|each| {
//                     let collection_id = each.0.id;
//                     let weak = weak.clone();

//                     div()
//                         .child(each.0.name)
//                         .on_mouse_up(MouseButton::Left, move |_event, _window, cx| {
//                             if let Some(workspace) = weak.upgrade() {
//                                 workspace.update(cx, |ws, cx| {
//                                     ws.on_collection_selected(collection_id, cx);
//                                 });
//                             }
//                         })
//                         .children(
//                             each.1
//                                 .into_iter()
//                                 .map(|view| div().child(view.name))
//                                 .collect::<Vec<_>>(),
//                         )
//                 })
//                 .collect::<Vec<_>>(),
//         )
//     }
// }

// // impl RenderOnce for CollectionList {
// //     fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
// //         div().children(
// //             self.collections
// //                 .into_iter() // turn Vec into iterator
// //                 .map(|each| {
// //                     div()
// //                         .child(each.0.name)
// //                         .on_mouse_up(MouseButton::Left, listener)
// //                         .children(each.1.into_iter().map(|view| div().child(view.name))) // make an element per item
// //                 })
// //                 .collect::<Vec<_>>(), // collect into Vec<impl IntoElement>
// //         )
// //     }
// // }
