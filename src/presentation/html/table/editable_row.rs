use maud::{Markup, PreEscaped, html};

pub async fn presentation_html_table_editable_row(
    table: String,
    id: String,
    column: String,
    value: String,
    search: Option<String>,
) -> Markup {
    html!(
        td {
            form
                method="post"
                action=(format!("/table/{}/{}/{}", table, id, column))
                hx-patch=(format!("/table/{}/{}/{}", table, id, column))
                hx-target="#main"
            {
                (PreEscaped(format!(
                                r#"<textarea
                                    id="editable_textarea"
                                    name="value"
                                    autofocus
                                    class="autosize-textarea"
                                    oninput="this.style.height='auto';this.style.height=(this.scrollHeight)+'px';"
                                    data-bind:search
                                    data-on:input__debounce.300ms="@get('/karma/{}')">{}</textarea>"#,
                                search.unwrap_or("foo".to_string()),&value
                            )))

                script { (PreEscaped(r#"(function(){var el=document.getElementById('editable_textarea'); if(!el) return; el.style.height='auto'; el.style.height=el.scrollHeight+'px';})();"#)) }
                button type="submit" { "Save" }
            }
        }
    )
}
