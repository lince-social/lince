use super::nav::presentation_web_section_nav;
use crate::presentation::web::collection::presentation_web_collection;

pub async fn header() -> String {
    "<header>".to_string()
        + presentation_web_section_nav().await.as_str()
        + presentation_web_collection().await.0.as_str()
        + "</header>"
}
