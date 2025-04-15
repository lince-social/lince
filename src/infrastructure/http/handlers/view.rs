use axum::extract::Path;

use crate::{
    application::providers::view::toggle::provider_view_toggle,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn handler_view_toggle(Path(id): Path<String>) -> String {
    let _ = provider_view_toggle(id).await;
    presentation_web_section_body().await
}
