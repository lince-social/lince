use axum::response::Html;

pub async fn header() -> Html<String> {
    Html(
        r#"
        <header>
        <nav
        hx-get="/section/nav"
        hx-trigger="load"
        hx-swap="outerHTML"
        ></nav>
        <div
        hx-get="/configuration/unhovered"
        hx-trigger="load"
        hx-swap="outerHTML"
        ></div>
        </header>
        "#
        .to_string(),
    )
}
