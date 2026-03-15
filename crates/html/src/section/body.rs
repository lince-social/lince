use super::main::presentation_html_section_main;
use crate::section::header::presentation_html_section_header;
use injection::cross_cutting::InjectedServices;

pub async fn presentation_html_section_body(services: InjectedServices) -> String {
    r#"<body id="body">"#.to_string()
        + r#"<div id="active-context-sse" data-init="@get('/sse/active-context', {openWhenHidden: true})"></div>"#
        + presentation_html_section_header(services.clone())
            .await
            .as_str()
        + presentation_html_section_main(services).await.as_str()
        + "</body>"
}

pub async fn presentation_html_section_body_home_modal(
    services: InjectedServices,
    element: String,
) -> String {
    format!(
        r##"
            <body id="body">
            <div id="active-context-sse" data-init="@get('/sse/active-context', {{openWhenHidden: true}})"></div>
            {header}
             {main}
              <div class="shy modal filled" data-on:keyup__window="if (evt.key === 'Escape') @get('/body')">
              {element}
              </div>
            </body>
        "##,
        header = presentation_html_section_header(services.clone()).await,
        main = presentation_html_section_main(services).await,
    )
}
