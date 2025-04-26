use maud::html;

use crate::presentation::web::section::body::presentation_web_section_body_nested_with_nav;

use super::selections::{
    presentation_web_karma_condition, presentation_web_karma_consequence,
    presentation_web_karma_karma,
};

pub async fn presentation_web_karma_orchestra() -> String {
    let page = "karma".to_string();
    let element = html!({
        div class="row" {
            (presentation_web_karma_condition(page.clone()).await)
            (presentation_web_karma_consequence(page.clone()).await)
            (presentation_web_karma_karma(page).await)
        }
    })
    .0;
    presentation_web_section_body_nested_with_nav(element).await
}
