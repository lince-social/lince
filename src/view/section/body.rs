use axum::response::{Html, IntoResponse};

pub async fn body() -> impl IntoResponse {
    Html(format!(
        r#"
        <body id="body">
        <main id="main"
        hx-trigger="load"
        hx-get="/section/main"
        hx-target="main"
        ></main>
        </body>
        "#
    ))
}
