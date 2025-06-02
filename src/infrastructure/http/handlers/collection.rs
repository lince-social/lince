use crate::application::use_cases::collection::set_active::use_case_collection_set_active;
use axum::{extract::Path, response::Html};

pub async fn handler_collection_set_active(Path(id): Path<String>) -> Html<String> {
    Html(use_case_collection_set_active(id).await.to_string())
}
