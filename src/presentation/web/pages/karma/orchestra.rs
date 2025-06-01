use super::collection::{
    presentation_web_karma_condition, presentation_web_karma_consequence,
    presentation_web_karma_karma,
};
use maud::html;

pub async fn presentation_web_karma_orchestra() -> String {
    html!({
        div class="row" {
            (presentation_web_karma_condition().await)
            (presentation_web_karma_consequence().await)
        }
           div { (presentation_web_karma_karma().await)}
    })
    .0
}
