use crate::presentation::web::operation::get::presentation_web_operation_get_operation_input;

pub async fn presentation_web_section_nav() -> String {
    r#"<nav class="row">"#.to_string() + presentation_web_operation_get_operation_input() + "</nav>"
}
