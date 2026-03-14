use crate::table::cell_id;
use maud::{Markup, PreEscaped, html};

pub async fn presentation_html_table_editable_row(
    table: String,
    id: String,
    column: String,
    value: String,
    search: Option<String>,
) -> Markup {
    let cell_id = cell_id(&table, &id, &column);
    html!(
        td.modal id=(cell_id) {
            form
                data-on:submit__prevent=(format!(
                    "@patch('/table/{}/{}/{}', {{contentType: 'form'}})",
                    table, id, column
                ))
            {
                (PreEscaped(format!(
                                r#"<textarea
                                    id="editable_textarea"
                                    name="value"
                                    autofocus
                                    class="autosize-textarea"
                                    oninput="this.style.height='auto';this.style.height=(this.scrollHeight)+'px';">{}</textarea>"#,
                                &value
                            )))

                button type="submit" { "Save" }
                @if let Some(search) = search {
                    input type="hidden" name="search" value=(search) {}
                }
            }
        }
    )
}
