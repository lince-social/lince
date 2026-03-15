use maud::{Markup, html};

pub async fn presentation_html_table_editable_row(
    table: String,
    id: String,
    column: String,
    value: String,
    search: Option<String>,
) -> Markup {
    html!(
        .column.s_gap.m_padding.table-editor-modal {
            h3 class="stripped" { "Edit Cell" }
            p class="stripped breakword" { (format!("{table}.{column} #{id}")) }
            form
                class="column s_gap"
                data-on:submit__prevent=(format!(
                    "@patch('/table/{}/{}/{}', {{contentType: 'form'}})",
                    table, id, column
                ))
            {
                textarea
                    id="editable_textarea"
                    name="value"
                    autofocus
                    class="autosize-textarea"
                    oninput="this.style.height='auto';this.style.height=(this.scrollHeight)+'px';"
                { (value) }
                .row.s_gap {
                    button type="submit" { "Save" }
                    button type="button" data-on:click="@get('/body')" { "Cancel" }
                }
                @if let Some(search) = search {
                    input type="hidden" name="search" value=(search) {}
                }
            }
        }
    )
}
