use maud::{Markup, html};

pub fn presentation_html_table_add_row(table: String) -> Markup {
    html!(
       button #button-add-row.s_padding type="button"
            data-on:click=(format!("@get('/operation/create/{table}')"))
            {"+"}
    )
}
