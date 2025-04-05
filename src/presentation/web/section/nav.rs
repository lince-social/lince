use axum::response::Html;

pub async fn nav() -> Html<String> {
    Html(
        r##"
            <form
            id="create_record_form"
            hx-post="/record/rada"
            hx-target="#body"
        >
            <input name="head" placeholder="Head">
            <button type="submit" style="display: none;"></button>
            </form>
        "##
        // <input name="quantity" placeholder="Quantity">
        // <input name="body" placeholder="Body">
        .to_string(),
    )
}

// <nav>
//     <div
//     id="operation"
//     hx-get="/operation"
//     hx-trigger="load"
//     hx-swap="outerHTML"
//     ></div>
//     <div
//     id="configurationunhovered"
//     hx-get="/configuration/unhovered"
//     hx-trigger="load"
//     hx-swap="outerHTML"
//     ></div>
// </nav>
