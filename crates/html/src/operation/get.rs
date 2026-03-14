use domain::dirty::operation::{DatabaseTable, OperationActions};
use maud::{Markup, html};

pub fn presentation_html_operation_get_operation_input() -> String {
    html! {
        div data-on:keyup__window="if (evt.key === 'Escape') document.getElementById('operation_input').style.display = 'none'" {
            form.northeast_modal.filled
                id="operation_input"
                data-on:submit__prevent="@post('/operation', {contentType: 'form'})"
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
                button
                    type="button"
                    style="display: none;"
                    data-on:click="document.getElementById('operation_input').style.display = 'none'" {}
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
    let operation_tables = [
        DatabaseTable::Configuration,
        DatabaseTable::Collection,
        DatabaseTable::View,
        DatabaseTable::CollectionView,
        DatabaseTable::Record,
        DatabaseTable::KarmaCondition,
        DatabaseTable::KarmaConsequence,
        DatabaseTable::Karma,
        DatabaseTable::Command,
        DatabaseTable::Frequency,
        DatabaseTable::Sum,
        DatabaseTable::History,
        DatabaseTable::DNA,
        DatabaseTable::Transfer,
    ]
    .into_iter()
    .map(|table| (table as usize, table.as_table_name()));
    let operation_actions = [
        (OperationActions::Create as usize, "create"),
        (OperationActions::SQLQuery as usize, "query"),
        (OperationActions::Karma as usize, "karma"),
        (OperationActions::Command as usize, "command"),
        (
            OperationActions::ActivateConfiguration as usize,
            "configuration",
        ),
    ];
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
