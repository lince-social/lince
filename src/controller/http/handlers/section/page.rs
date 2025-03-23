use axum::response::{Html, IntoResponse};

pub async fn page_handler() -> impl IntoResponse {
    Html(
        br#"
    <!doctype html>
    <html lang="en">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1.0" />
            <meta http-equiv="X-UA-Compatible" content="ie=edge" />
            <title>Lince</title>
            <script src="https://unpkg.com/htmx.org@2.0.4"></script>
            <style>
                body {
                    background-color: #000000;
                    color: #ffffff;
                }
            </style>
        </head>
        <body
            id="body"
            hx-trigger="load"
            hx-get="/section/body"
            hx-swap="outerHTML"
        ></body>
    </html>
    "#,
    )
}
