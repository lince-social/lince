use axum::response::Html;

pub async fn header() -> Html<String> {
    Html(
        r#"
        <header
        id="header"
        hx-get="/section/nav"
        hx-trigger="load"
        hx-swap="innerHTML"
        ></header>
        "#
        .to_string(),
    )
}
