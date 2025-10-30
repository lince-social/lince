use maud::{Markup, PreEscaped, html};

pub async fn presentation_html_table_editable_row(
    table: String,
    id: String,
    column: String,
    value: String,
    _search: Option<String>,
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
                        oninput="this.style.height='auto';this.style.height=(this.scrollHeight)+'px';">{}</textarea>
                        <script>
                        (function(){{
                            var el = document.getElementById('editable_textarea');
                            if(!el) return;
                            // simple debounce
                            var timeout = null;
                            el.addEventListener('input', function(e){{
                                clearTimeout(timeout);
                                timeout = setTimeout(function(){{
                                    try {{
                                        fetch('/karma/condition', {{
                                            method: 'POST',
                                            headers: {{ 'Content-Type': 'application/json' }},
                                            body: JSON.stringify({{ search: el.value }})
                                        }}).catch(function(err){{ console.log('search post failed', err); }});
                                    }} catch(err) {{
                                        console.log('search post failed', err);
                                    }}
                                }}, 300);
                            }});
                        }})();
                        </script>"#,
                (value)
                )))

                script {
                    (PreEscaped(r#"
                        (function(){
                            var el=document.getElementById('editable_textarea');
                            if(!el) return;
                            el.style.height='auto';
                            el.style.height=el.scrollHeight+'px';
                        })();
                    "#))
                }

                button type="submit" { "Save" }
            }

        }
    )
}
