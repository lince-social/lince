use axum::response::Html;

use crate::presentation::web::section::main::main_component;

pub async fn main_handler() -> Html<String> {
    Html(main_component().await.0)
}
