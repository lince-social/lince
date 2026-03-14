use super::nav::presentation_html_section_nav;
use crate::collection::presentation_html_collection;
use injection::cross_cutting::InjectedServices;

pub async fn presentation_html_section_header(services: InjectedServices) -> String {
    r#"<header id="header">"#.to_string()
        + presentation_html_section_nav().await.as_str()
        + presentation_html_collection(services).await.0.as_str()
        + r#"<div id="karma-search-modal"></div>"#
        + "</header>"
}
