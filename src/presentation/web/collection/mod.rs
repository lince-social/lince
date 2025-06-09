use crate::{
    application::{
        schema::{collection::row::ConfigurationForBarScheme, view::queried_view::QueriedView},
        use_cases::collection::{
            get_active::use_case_collection_get_active,
            get_inactive::use_case_collection_get_inactive,
        },
    },
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::view::toggle_all::presentation_web_view_toggle_all,
};
use maud::{Markup, html};

pub async fn presentation_web_collection(services: InjectedServices) -> Markup {
    let (active_collection, active_collection_views) =
        services.providers.collection.get_active().await;
    let inactive_collections = services.providers.collection.get_inactive().await;
    html!(
        .configurations.column.xs_gap
            data-signals="{configurationOpen: false}"
            data-on-mouseover="$configurationOpen = true"
            data-on-mouseleave="$configurationOpen = false"
        {
            (presentation_web_collection_row(active_collection, active_collection_views).await)
            .inactive_configurations.column.xs_gap data-show="$configurationOpen" {
                @for (inactive_collection, inactive_collection_views) in inactive_collections {
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
    .row.xs_gap {
             @if collection.quantity == 1 {
                button.active
                    {(collection.name)}
             } @else {
                 button.inactive
                    hx-patch=(format!("/collection/active/{}", collection.id))
                    hx-trigger="click"
                    hx-target="#body"
                    {(collection.name)}
             }
             (presentation_web_view_toggle_all(collection.id).await)
         @for view in views {
             @if view.quantity == 1 {
                 button.active
                     hx-patch=(format!("/view/toggle/{}/{}", collection.id, view.id))
                     hx-target="#body"
                    {(view.name)}
             } @else {
                 button.inactive
                     hx-patch=(format!("/view/toggle/{}/{}", collection.id, view.id))
                     hx-target="#body"
                    {(view.name)}
             }
         }
     }
    )
}
