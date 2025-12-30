use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::gpui::{
        components::parts::collection::bar::{CollectionList, CollectionSelectedEvent},
        state::State,
    },
};
use gpui::*;

pub struct Workspace {
    pub focus_handle: FocusHandle,
    pub state: State,
    pub services: InjectedServices,
    pub collection_list: Entity<CollectionList>,
    pub _subscriptions: Vec<Subscription>,
}

impl Workspace {
    fn new(cx: &mut Context<Self>, services: InjectedServices, state: State) -> Self {
        let collection_list = CollectionList::view(cx, state.collections.clone());

        let _subscriptions = vec![cx.subscribe(
            &collection_list,
            |this: &mut Workspace, _, event: &CollectionSelectedEvent, cx| {
                this.on_collection_selected(event.id, cx);
            },
        )];

        Self {
            focus_handle: cx.focus_handle(),
            state,
            services,
            collection_list,
            _subscriptions,
        }
    }

    pub fn view(cx: &mut App, services: InjectedServices, state: State) -> Entity<Self> {
        cx.new(|cx| Self::new(cx, services, state))
    }

    pub fn on_collection_selected(&mut self, collection_id: u32, cx: &mut Context<Self>) {
        let services = self.services.clone();

        cx.spawn(async move |this, cx| {
            let _ = services
                .repository
                .collection
                .set_active(&collection_id.to_string())
                .await;

            let rows = services
                .repository
                .collection
                .get_all()
                .await
                .unwrap_or_else(|_| vec![]);

            cx.update_global::<CollectionList, _>(|list, _| {
                list.collections = rows;
            });

            cx.notify();
        })
        .detach();
    }

    // pub fn on_collection_selected(&mut self, collection_id: u32, cx: &mut Context<Self>) {
    //     let services = self.services.clone();
    //     cx.spawn(async move |this, cx| {
    //         if let Err(e) = services
    //             .repository
    //             .collection
    //             .set_active(&collection_id.to_string())
    //             .await
    //         {
    //             log!(e, "failed to set active collection");
    //         }

    //         let rows = match services.repository.collection.get_all().await {
    //             Ok(rows) => rows,
    //             Err(e) => {
    //                 log!(e, "failed fetch");
    //                 vec![(Collection::error(), Vec::new())]
    //             }
    //         };

    //         this.update(cx, move |owner, cx| {
    //             owner.state.collections = rows.clone();

    //             owner.collection_list.update(cx, move |bar, _| {
    //                 bar.collections = rows.clone();
    //             });

    //             cx.notify();
    //         })
    //         .unwrap();
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
