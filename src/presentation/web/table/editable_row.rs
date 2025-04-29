use maud::{Markup, html};

pub async fn presentation_web_table_editable_row(
    table: String,
    id: String,
    column: String,
    value: String,
) -> Markup {
    html!(td {div hx-target="#main"
        hx-trigger="keyup[key === 'Escape'] from:body"
        hx-get="/section/main"
        { input
        type="text"
        name="value"
        hx-params="*"
        hx-target="#main"
        hx-patch=(format!("/table/{}/{}/{}", table, id, column)) value=(value.replace("'", "''")){}}})
}
