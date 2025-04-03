use axum::response::{Html, IntoResponse};

pub async fn body() -> impl IntoResponse {
    Html(
        r#"
        <body id="body">
        <header
        id="header"
         hx-get="/section/header"
         hx-trigger="load"
         hx-swap="outerHTML"
         ></header>
         <main
         id="main"
          hx-get="/section/main"
          hx-trigger="load"
          hx-swap="outerHTML"
          ></main>
        </body>
        "#,
    )
}

pub async fn nested_body(element: String) -> Html<String> {
    Html(format!(
        r#"
            <body id="body">
            <header
            id="header"
             hx-get="/section/header"
             hx-trigger="load"
             hx-swap="outerHTML"
             ></header>
             <main
             id="main"
              hx-get="/section/main"
              hx-trigger="load"
              hx-swap="outerHTML"
              ></main>
              {element}
            </body>
        "#
    ))
}
