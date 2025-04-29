use crate::{
    application::{
        schema::view::queried_view::QueriedView,
        use_cases::configuration::get_active::use_case_configuration_get_active,
    },
    domain::entities::configuration::Configuration,
    presentation::web::view::toggle_all::presentation_web_view_toggle_all,
};
use maud::{Markup, html};

pub async fn presentation_web_configuration_unhovered() -> Markup {
    let (active_configuration, active_configuration_views) =
        use_case_configuration_get_active().await;
    html!(div
    class="framed row"
    hx-get="/configuration/hovered"
     hx-trigger="mouseenter"
     hx-swap="outerHTML"
    { button class="active"
        {(active_configuration.name)}
    (presentation_web_view_toggle_all(active_configuration.id).await)
        @for active_configuration_view in active_configuration_views {
           @if active_configuration_view.quantity == 1 {
               button hx-patch=(format!("/view/toggle/view/{}", active_configuration_view.id)) hx-target="#body" class="active"
               {(active_configuration_view.name)}
           } @else {
               button hx-patch=(format!("/view/toggle/view/{}", active_configuration_view.id)) hx-target="#body" class="inactive"
               {(active_configuration_view.name)}
           }
        }
    })
}

pub async fn presentation_web_configuration_hovered(
    active_configuration: Configuration,
    active_configuration_views: Vec<QueriedView>,
    inactive_configurations: Vec<Configuration>,
) -> Markup {
    html!(
        div
        hx-get="/configuration/unhovered"
         hx-trigger="mouseleave"
         hx-swap="outerHTML"
            {
           div
            class="framed row"
                { button class="active" {(active_configuration.name)}
                    (presentation_web_view_toggle_all(active_configuration.id).await)
                @for active_configuration_view in active_configuration_views {
                    @if active_configuration_view.quantity == 1 {
                        button hx-patch=(format!("/view/toggle/view/{}", active_configuration_view.id)) hx-target="#body" class="active"
                        {(active_configuration_view.name)}
                    } @else {
                        button hx-patch=(format!("/view/toggle/view/{}", active_configuration_view.id)) hx-target="#body" class="inactive"
                        {(active_configuration_view.name)}
                    }
                }
            }
        div
        { @for inactive_configuration in inactive_configurations {
            div class="framed row" {
                button
                    class="inactive"
                    hx-patch=(format!("/configuration/active/{}", inactive_configuration.id))
                    hx-trigger="click"
                    hx-target="#body"
                {(inactive_configuration.name)}
                (presentation_web_view_toggle_all(active_configuration.id).await)
                }
        } }
        }

    )
}
