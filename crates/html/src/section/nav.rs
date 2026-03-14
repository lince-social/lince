use crate::presentation::html::operation::get::presentation_html_operation_get_operation_input;

pub async fn presentation_html_section_nav() -> String {
    r#"<nav class="row">"#.to_string()
        + presentation_html_operation_get_operation_input().as_str()
        + "</nav>"
}
