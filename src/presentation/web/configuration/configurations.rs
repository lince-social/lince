use crate::{
    application::{
        schema::{configuration::row::ConfigurationForBarScheme, view::queried_view::QueriedView},
        use_cases::configuration::{
            get_active::use_case_configuration_get_active,
            get_inactive::use_case_configuration_get_inactive,
        },
    },
    presentation::web::view::toggle_all::presentation_web_view_toggle_all,
};
use maud::{Markup, html};

pub async fn presentation_web_configuration_unhovered() -> Markup {
    let (active_configuration, active_configuration_views) =
        use_case_configuration_get_active().await;

    html!(
        div
            hx-get="/configuration/hovered"
             hx-trigger="mouseenter"
             hx-swap="outerHTML"
             {
        (presentation_web_configuration_row(active_configuration, active_configuration_views)
            .await)
                 }
    )
}

pub async fn presentation_web_configuration_hovered() -> Markup {
    let (active_configuration, active_configuration_views) =
        use_case_configuration_get_active().await;
    let inactive_configurations = use_case_configuration_get_inactive().await;
    html!(
        div
        hx-get="/configuration/unhovered"
         hx-trigger="mouseleave"
         hx-swap="outerHTML"
            {
                (presentation_web_configuration_row(active_configuration, active_configuration_views).await)
        div
        { @for (inactive_configuration, inactive_configuration_views) in inactive_configurations {
           (presentation_web_configuration_row(inactive_configuration, inactive_configuration_views).await)
        }
        }
        }
    )
}

async fn presentation_web_configuration_row(
    configuration: ConfigurationForBarScheme,
    views: Vec<QueriedView>,
) -> Markup {
    html!(
    div
     class="framed row"
         {
             @if configuration.quantity == 1 {
                button class="active"
                {(configuration.name)}
             } @else {
                 button class="inactive"
                    hx-patch=(format!("/configuration/active/{}", configuration.id))
                    hx-trigger="click"
                    hx-target="#body"
                 {(configuration.name)}
             }
             (presentation_web_view_toggle_all(configuration.id).await)
         @for view in views {
             @if view.quantity == 1 {
                 button hx-patch=(format!("/view/toggle/view/{}", view.id)) hx-target="#body" class="active"
                 {(view.name)}
             } @else {
                 button hx-patch=(format!("/view/toggle/view/{}", view.id)) hx-target="#body" class="inactive"
                 {(view.name)}
             }
         }
     }
    )
}
