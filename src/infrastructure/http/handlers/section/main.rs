use axum::response::{Html, IntoResponse};

pub async fn main_handler() -> impl IntoResponse {
    Html(format!(
        r#"
        <p hx-get="/configuration" hx-trigger="load" hx-swap="outerHTML"></p>
        "#
    ))
}
