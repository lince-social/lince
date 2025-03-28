use axum::response::{Html, IntoResponse};

pub async fn main() -> impl IntoResponse {
    Html(format!(
        r#"
        <p hx-get="/configuration/unhovered" hx-trigger="load" hx-swap="outerHTML"></p>
        "#
    ))
}
