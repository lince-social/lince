use crate::application::operation::{operation_actions, operation_tables};
use maud::{Markup, html};

pub fn presentation_html_operation_get_operation_input() -> String {
    html! {
        div
            hx-target="#body"
            hx-trigger="keyup[key === 'Escape'] from:body"
            hx-get="/body"
        {
            form.modal
                id="operation_input"
                hx-post="/operation"
                hx-target="#body"
                hx-on::after-request="if(event.detail.successful) this.reset(); this.style.display = 'none';"
                style="display: none;"
            {
                input
                    class="filled"
                    id="operation_input_field"
                    name="operation"
                    autofocus;
                button
                    type="submit"
                    style="display: none;" {}
                (presentation_html_get_operation_options())
            }
        }
        script {
            (maud::PreEscaped(r#"
                document.addEventListener('keydown', function (e) {
                    const tag = document.activeElement.tagName.toLowerCase();
                    if ((tag === 'input' || tag === 'textarea' || document.activeElement.isContentEditable)) return;

                    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
                        const form = document.getElementById('operation_input');
                        const input = document.getElementById('operation_input_field');
                        if (form.style.display === 'none') {
                            form.style.display = 'block';
                            input.focus();
                            input.value = e.key;
                            input.setSelectionRange(input.value.length, input.value.length);
                            e.preventDefault();
                        }
                    }
                });
            "#))
        }

    }.0
}

pub fn presentation_html_get_operation_options() -> Markup {
    let operation_tables = operation_tables();
    let operation_actions = operation_actions();
    html!(
        .filled.row.s_margin {
            .column {
                @for (table_number, table_name) in operation_tables {
                    .row.xs_gap {
                        div {(table_number)}
                        div {(table_name)}
                    }
                }
           }
           .column {
                @for (action_number, action_name) in operation_actions {
                    .row.xs_gap {
                        div {(action_number)}
                        div {(action_name)}
                    }
                }
           }
        }
    )
}
