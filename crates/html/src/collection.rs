use crate::view::toggle_all::presentation_html_view_toggle_all;
use domain::{clean::collection::Collection, dirty::view::QueriedView};
use injection::cross_cutting::InjectedServices;
use maud::{Markup, html};

pub async fn presentation_html_collection(services: InjectedServices) -> Markup {
    let opt = services
        .repository
        .collection
        .get_active()
        .await
        .map_err(|e| html!((format!("Failed to get active collection. Error: {}", e))))
        .unwrap();
    if opt.is_none() {
        return html!("No active collection");
    }
    let (active_collection_name, active_collection_views) = opt.unwrap();

    //help sort by collection with smallest id first
    let mut inactive_collections: Vec<(Collection, Vec<QueriedView>)> = services
        .repository
        .collection
        .get_inactive()
        .await
        .map_err(|e| html!((format!("Failed to get active collection. Error: {}", e))))
        .unwrap();
    inactive_collections.sort_by_key(|(collection, _)| collection.id);

    html!(
        .configurations.column.xs_gap
        data-signals="{configurationOpen: false}"
        data-on:mouseover="$configurationOpen = true"
        data-on:mouseleave="$configurationOpen = false" {
            (presentation_html_collection_row(active_collection_name, active_collection_views).await)
            .inactive_configurations.column.xs_gap data-show="$configurationOpen" {
                @for (inactive_collection, inactive_collection_views) in inactive_collections {
                    (presentation_html_collection_row(inactive_collection, inactive_collection_views).await)
                }
            }
        }
    )
}

async fn presentation_html_collection_row(
    collection: Collection,
    views: Vec<QueriedView>,
) -> Markup {
    html!(
        .row.xs_gap {
            button #button-collection-id {(collection.id)}
            @if collection.quantity == 1 {
                button.active {(collection.name)}
            } @else {
                button.inactive
                    type="button"
                    data-on:click=(format!("@patch('/collection/active/{}')", collection.id))
                    {(collection.name)}
            }
            (presentation_html_view_toggle_all(collection.id).await)
            @for view in views {
                @if view.quantity == 1 {
                    button.active
                        type="button"
                        data-on:click=(format!("@patch('/view/toggle/{}/{}')", collection.id, view.id))
                        {(view.name)}
                } @else {
                    button.inactive
                        type="button"
                        data-on:click=(format!("@patch('/view/toggle/{}/{}')", collection.id, view.id))
                        {(view.name)}
                }
            }
        }
    )
}
