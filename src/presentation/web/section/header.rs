use crate::presentation::web::selection::presentation_web_selection_unhovered;

use super::nav::presentation_web_section_nav;

pub async fn header() -> String {
    "<header>".to_string()
        + presentation_web_section_nav().await.as_str()
        + presentation_web_selection_unhovered().await.0.as_str()
        + "</header>"
}
