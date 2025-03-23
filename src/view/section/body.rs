use axum::response::{Html, IntoResponse};

pub async fn body() -> impl IntoResponse {
    // <main id="main"
    // hx-trigger="load"
    // hx-get="/section/main"
    // hx-target="main"
    // ></main>
    Html(format!(
        r#"
        <body id="body">
        <p>test</p>
        </body>
        "#
    ))
}
