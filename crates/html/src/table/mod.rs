pub mod add_row;
pub mod editable_row;
pub mod tables;

pub fn wrap_main(contents: String) -> String {
    format!(r#"<main id="main">{contents}</main>"#)
}

pub fn cell_id(table: &str, id: impl std::fmt::Display, column: &str) -> String {
    fn sanitize(value: &str) -> String {
        value
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect()
    }

    format!(
        "cell-{}-{}-{}",
        sanitize(table),
        sanitize(&id.to_string()),
        sanitize(column)
    )
}
