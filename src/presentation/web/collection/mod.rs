use crate::{
    application::{
        schema::{collection::row::ConfigurationForBarScheme, view::queried_view::QueriedView},
        use_cases::collection::{
            get_active::use_case_collection_get_active,
            get_inactive::use_case_collection_get_inactive,
        },
    },
    presentation::web::view::toggle_all::presentation_web_view_toggle_all,
};
use maud::{Markup, html};

pub async fn presentation_web_collection_unhovered() -> Markup {
    let (active_collection, active_collection_views) = use_case_collection_get_active().await;

    html!(
        div
            hx-get="/collection/hovered"
             hx-trigger="mouseenter"
             hx-swap="outerHTML"
             {
        (presentation_web_collection_row(active_collection, active_collection_views)
            .await)
                 }
    )
}

pub async fn presentation_web_collection_hovered() -> Markup {
    let (active_collection, active_collection_views) = use_case_collection_get_active().await;
    let inactive_collections = use_case_collection_get_inactive().await;
    html!(
        div
        hx-get="/collection/unhovered"
         hx-trigger="mouseleave"
         hx-swap="outerHTML"
            {
                (presentation_web_collection_row(active_collection, active_collection_views).await)
        div
        { @for (inactive_collection, inactive_collection_views) in inactive_collections {
           (presentation_web_collection_row(inactive_collection, inactive_collection_views).await)
        }
        }
        }
    )
}

async fn presentation_web_collection_row(
    collection: ConfigurationForBarScheme,
    views: Vec<QueriedView>,
) -> Markup {
    html!(
    div
     class="row"
         {
             @if collection.quantity == 1 {
                button class="active"
                {(collection.name)}
             } @else {
                 button class="inactive"
                    hx-patch=(format!("/collection/active/{}", collection.id))
                    hx-trigger="click"
                    hx-target="#body"
                 {(collection.name)}
             }
             (presentation_web_view_toggle_all(collection.id).await)
         @for view in views {
             @if view.quantity == 1 {
                 button hx-patch=(format!("/view/toggle/{}/{}", collection.id, view.id)) hx-target="#body" class="active"
                 {(view.name)}
             } @else {
                 button hx-patch=(format!("/view/toggle/{}/{}", collection.id, view.id)) hx-target="#body" class="inactive"
                 {(view.name)}
             }
         }
     }
    )
}
