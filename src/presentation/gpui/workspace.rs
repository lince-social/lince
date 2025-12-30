use crate::{
    domain::clean::collection::Collection,
    infrastructure::cross_cutting::InjectedServices,
    log,
    presentation::gpui::{components::parts::collection::bar::CollectionList, state::State},
};
use gpui::*;

pub struct Workspace {
    pub focus_handle: FocusHandle,
    pub state: State,
    pub services: InjectedServices,
    pub collection_list: Entity<CollectionList>,
}

impl Workspace {
    fn new(cx: &mut Context<Self>, services: InjectedServices, state: State) -> Self {
        let weak = cx.weak_entity();
        let collection_list = cx.new(|_| CollectionList::new(state.collections.clone(), weak));

        Self {
            focus_handle: cx.focus_handle(),
            state,
            services,
            collection_list,
        }
    }

    pub fn view(cx: &mut App, services: InjectedServices, state: State) -> Entity<Self> {
        cx.new(|cx| Self::new(cx, services, state))
    }
    pub fn on_collection_selected(&mut self, collection_id: u32, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = services
                .repository
                .collection
                .set_active(&collection_id.to_string())
                .await
            {
                log!(e, "failed to set active collection");
            }

            let rows = match services.repository.collection.get_all().await {
                Ok(rows) => rows,
                Err(e) => {
                    log!(e, "failed fetch");
                    vec![(Collection::error(), Vec::new())]
                }
            };

            this.update(cx, move |owner, cx| {
                owner.state.collections = rows.clone();

                owner.collection_list.update(cx, move |bar, _| {
                    bar.collections = rows.clone();
                });

                cx.notify();
            })
            .unwrap();
        })
        .detach();
    }

    // pub fn on_collection_selected(&mut self, collection_id: u32, cx: &mut Context<Self>) {
    //     let services = self.services.clone();

    //     cx.spawn(async move |_this, cx| {
    //         let _ = services
    //             .repository
    //             .collection
    //             .set_active(&collection_id.to_string())
    //             .await;

    //         let rows = services
    //             .repository
    //             .collection
    //             .get_all()
    //             .await
    //             .unwrap_or_else(|_| vec![]);

    //         cx.update_global::<CollectionList, _>(|list, _| {
    //             list.collections = rows;
    //         });

    //         // cx.notify();
    //     })
    //     .detach();
    // }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .bg(rgb(0x1a1a1a))
            .size_full()
            .p_6()
            .track_focus(&self.focus_handle(cx))
            .child(self.collection_list.clone())
            .text_color(rgb(0xffffff))
            .child("Hellour")
    }
}
