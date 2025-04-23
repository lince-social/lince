use crate::presentation::web::operation::get::presentation_web_operation_get_operation_input;

pub async fn presentation_web_section_nav() -> String {
    r#"<nav class="row">"#.to_string()
        + presentation_web_nav_home()
        + presentation_web_nav_karma_orchestra()
        + presentation_web_operation_get_operation_input()
        + "</nav>"
}

pub fn presentation_web_nav_home() -> &'static str {
    r##"<button hx-get="/" hx-target="#body">Home</button>"##
}

pub fn presentation_web_nav_karma_orchestra() -> &'static str {
    r##"<button hx-get="/page/karma_orchestra" hx-target="#body">Karma Orchestra</button>"##
}
