use crate::presentation::web::table::tables::presentation_web_tables;

use super::header::header;

pub async fn presentation_web_section_body() -> String {
    r#"<body id="body">"#.to_string()
        + header().await.as_str()
        + presentation_web_tables().await.0.as_str()
        + "</body>"
}

pub async fn nested_body(element: String) -> String {
    format!(
        r##"
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
              <div class="framed shy modal filled"
              hx-get="/section/body"
              hx-trigger="keyup[key === 'Escape'] from:body"
              hx-target="#body"
              >
              {element}
              </div>
            </body>
        "##
    )
}
