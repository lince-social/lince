use crate::{
    application::schema::view::queried_view::QueriedView,
    domain::entities::configuration::Configuration,
};
use maud::{Markup, html};

pub async fn presentation_web_configuration_unhovered(
    active_configuration: Configuration,
    active_configuration_views: Vec<QueriedView>,
) -> Markup {
    html!(div
    class="framed"
    hx-get="/configuration/hovered"
     hx-trigger="mouseenter"
     hx-swap="outerHTML"
    { button class="active_configuration"
        {(active_configuration.name)}
        @for active_configuration_view in active_configuration_views {
           @if active_configuration_view.quantity == 1 {
               button hx-patch=(format!("/view/{}", active_configuration_view.id)) hx-target="#body" class="active_view"
               {(active_configuration_view.name)}
           } @else {
               button hx-patch=(format!("/view/{}", active_configuration_view.id)) hx-target="#body" class="inactive_view"
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
            class="framed"
                { button class="active_configuration" {(active_configuration.name)}
                @for active_configuration_view in active_configuration_views {
                    @if active_configuration_view.quantity == 1 {
                        button hx-patch=(format!("/view/{}", active_configuration_view.id)) hx-target="#body" class="active_view"
                        {(active_configuration_view.name)}
                    } @else {
                        button hx-patch=(format!("/view/{}", active_configuration_view.id)) hx-target="#body" class="inactive_view"
                        {(active_configuration_view.name)}
                    }
                }
            }
        div
        { @for inactive_configuration in inactive_configurations {
            div class="framed" {
                button
                    class="inactive_configuration"
                    hx-patch=(format!("/configuration/active/{}", inactive_configuration.id))
                    hx-trigger="click"
                    hx-target="#body"
                {(inactive_configuration.name)}}
        } }
        }

    )
}
