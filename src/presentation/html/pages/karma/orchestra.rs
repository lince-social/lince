use super::collection::{
    presentation_html_karma_condition, presentation_html_karma_consequence,
    presentation_html_karma_karma,
};
use crate::infrastructure::cross_cutting::InjectedServices;
use maud::html;

pub async fn presentation_html_karma_orchestra(services: InjectedServices) -> String {
    html!({
        div class="row" {
            (presentation_html_karma_condition(services.clone()).await)
            (presentation_html_karma_consequence(services.clone()).await)
        }
           div { (presentation_html_karma_karma(services).await)}
    })
    .0
}
