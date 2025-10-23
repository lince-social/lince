use maud::{Markup, PreEscaped, html};

pub async fn presentation_html_table_editable_row(
    table: String,
    id: String,
    column: String,
    value: String,
) -> Markup {
    html!(
        td {
            form
                method="post"
                action=(format!("/table/{}/{}/{}", table, id, column))
                hx-patch=(format!("/table/{}/{}/{}", table, id, column))
                hx-target="#main"
            {
                textarea id="editable_textarea" name="value" autofocus class="autosize-textarea"
                    oninput="this.style.height='auto'; this.style.height=(this.scrollHeight)+'px';"
                {
                    (value)
                }
                // small inline script to size the textarea immediately after it's inserted
                script { (PreEscaped(r#"(function(){var el=document.getElementById('editable_textarea'); if(!el) return; el.style.height='auto'; el.style.height=el.scrollHeight+'px';})();"#)) }
                button type="submit" { "Save" }
            }
        }
    )
}
