use maud::{Markup, html};

pub fn presentation_web_table_add_row(table: String) -> Markup {
    html!(
       button class="shy"
            hx-post="/operation"
            hx-target="#body"
            hx-params="*"
            name="operation"
            value=("create".to_string() + " " + table.as_str())
            {"+"}
    )
}
