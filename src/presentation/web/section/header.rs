use super::nav::presentation_web_section_nav;
use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::collection::presentation_web_collection,
};

pub async fn presentation_web_section_header(services: InjectedServices) -> String {
    "<header>".to_string()
        + presentation_web_section_nav().await.as_str()
        + presentation_web_collection(services).await.0.as_str()
        + "</header>"
}
