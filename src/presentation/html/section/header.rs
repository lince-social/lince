use super::nav::presentation_html_section_nav;
use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::collection::presentation_html_collection,
};

pub async fn presentation_html_section_header(services: InjectedServices) -> String {
    "<header>".to_string()
        + presentation_html_section_nav().await.as_str()
        + presentation_html_collection(services).await.0.as_str()
        + r#"<div id="karma-search-modal"></div>"#
        + "</header>"
}
