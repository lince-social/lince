use crate::{
    application::{
        schema::{selection::row::ConfigurationForBarScheme, view::queried_view::QueriedView},
        use_cases::selection::{
            get_active::use_case_selection_get_active,
            get_inactive::use_case_selection_get_inactive,
        },
    },
    presentation::web::view::toggle_all::presentation_web_view_toggle_all,
};
use maud::{Markup, html};

pub async fn presentation_web_selection_unhovered() -> Markup {
    let (active_selection, active_selection_views) = use_case_selection_get_active().await;

    html!(
        div
            hx-get="/selection/hovered"
             hx-trigger="mouseenter"
             hx-swap="outerHTML"
             {
        (presentation_web_selection_row(active_selection, active_selection_views)
            .await)
                 }
    )
}

pub async fn presentation_web_selection_hovered() -> Markup {
    let (active_selection, active_selection_views) = use_case_selection_get_active().await;
    let inactive_selections = use_case_selection_get_inactive().await;
    html!(
        div
        hx-get="/selection/unhovered"
         hx-trigger="mouseleave"
         hx-swap="outerHTML"
            {
                (presentation_web_selection_row(active_selection, active_selection_views).await)
        div
        { @for (inactive_selection, inactive_selection_views) in inactive_selections {
           (presentation_web_selection_row(inactive_selection, inactive_selection_views).await)
        }
        }
        }
    )
}

async fn presentation_web_selection_row(
    selection: ConfigurationForBarScheme,
    views: Vec<QueriedView>,
) -> Markup {
    html!(
    div
     class="framed row"
         {
             @if selection.quantity == 1 {
                button class="active"
                {(selection.name)}
             } @else {
                 button class="inactive"
                    hx-patch=(format!("/selection/active/{}", selection.id))
                    hx-trigger="click"
                    hx-target="#body"
                 {(selection.name)}
             }
             (presentation_web_view_toggle_all(selection.id).await)
         @for view in views {
             @if view.quantity == 1 {
                 button hx-patch=(format!("/view/toggle/{}/{}", selection.id, view.id)) hx-target="#body" class="active"
                 {(view.name)}
             } @else {
                 button hx-patch=(format!("/view/toggle/{}/{}", selection.id, view.id)) hx-target="#body" class="inactive"
                 {(view.name)}
             }
         }
     }
    )
}
