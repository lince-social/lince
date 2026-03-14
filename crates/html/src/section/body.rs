use super::main::presentation_html_section_main;
use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::section::header::presentation_html_section_header,
};

pub async fn presentation_html_section_body(services: InjectedServices) -> String {
    r#"<body id="body">"#.to_string()
        + presentation_html_section_header(services.clone())
            .await
            .as_str()
        + presentation_html_section_main(services).await.as_str()
        + "</body>"
}

pub async fn presentation_html_section_body_home_modal(element: String) -> String {
    format!(
        r##"
            <body id="body">
            <header
            id="header"
             hx-get="/header"
             hx-trigger="load"
             hx-swap="outerHTML"
             ></header>
             <main
             id="main"
              hx-get="/main"
              hx-trigger="load"
              hx-swap="outerHTML"
              ></main>
              <div class="shy modal filled"
              hx-get="/body"
              hx-trigger="keyup[key === 'Escape'] from:body"
              hx-target="#body"
              >
              {element}
              </div>
            </body>
        "##
    )
}
