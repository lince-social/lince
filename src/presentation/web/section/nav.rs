use axum::response::Html;

pub async fn nav() -> Html<String> {
    Html(
        r##"
<nav class="row">
    <div
    id="operation"
    hx-get="/operation"
    hx-trigger="load"
    hx-swap="outerHTML"
    ></div>
            <form
            id="create_record_form"
            hx-post="/record"
            hx-target="#main"
            hx-on::after-request="if(event.detail.successful) this.reset()"
        >
            <input name="head" placeholder="Head">
            <button type="submit" style="display: none;"></button>
            </form>
</nav>
        "##
        .to_string(),
        // <input name="quantity" placeholder="Quantity">
        // <input name="body" placeholder="Body">
    )
}
