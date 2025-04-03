use axum::response::Html;

pub async fn nav() -> Html<String> {
    Html(
        r#"
        <nav>
            <div
            id="operation"
            hx-get="/operation"
            hx-trigger="load"
            hx-swap="outerHTML"
            ></div>
            <div
            id="configurationunhovered"
            hx-get="/configuration/unhovered"
            hx-trigger="load"
            hx-swap="outerHTML"
            ></div>
        </nav>
        "#
        .to_string(),
    )
}
