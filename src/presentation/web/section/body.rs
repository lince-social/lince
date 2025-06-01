use super::{header::header, main::presentation_web_section_main};

pub async fn presentation_web_section_body() -> String {
    r#"<body id="body">"#.to_string()
        + header().await.as_str()
        + presentation_web_section_main().await.as_str()
        + "</body>"
}

pub async fn presentation_web_section_body_home_modal(element: String) -> String {
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
              <div class="shy modal filled"
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
