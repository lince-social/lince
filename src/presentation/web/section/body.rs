pub fn presentation_web_section_body() -> &'static str {
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
        "#
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
