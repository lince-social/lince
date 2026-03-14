use crate::table::cell_id;
use injection::cross_cutting::InjectedServices;
use maud::{Markup, html};
use utils::log;

pub async fn presentation_html_karma_view(services: InjectedServices) -> String {
    html!(
        .column {
            (presentation_html_karma(services.clone()).await)
            .row {
                (presentation_html_karma_view_condition(services.clone()).await)
                (presentation_html_karma_view_consequence(services.clone()).await)
            }
        }
    )
    .0
}

async fn presentation_html_karma(services: InjectedServices) -> Markup {
    match services.repository.karma.get_view().await {
        Ok(karma_views) => html! {
            div {
                p { "Karma View" }
                table class="rounded-table" {
                    thead {
                        tr {
                            th.karma-view-karma class="top-left" { "id" }
                            th.karma-view-karma { "Qty" }
                            th.karma-view-karma { "Name" }
                            th.karma-view-condition { "Kcd Qty" }
                            th.karma-view-condition { "Kcd Id" }
                            th.karma-view-condition { "Kcd Name" }
                            th.karma-view-condition { "Kcd Cd" }
                            th.karma-view-karma { "Operand" }
                            th.karma-view-condition { "Kcs Cs" }
                            th.karma-view-condition { "Kcs Name" }
                            th.karma-view-condition { "Kcs Id" }
                            th.karma-view-condition.top-right { "Kcs Qty" }
                        }
                    }
                    tbody {
                        @for (row_i, karma_view) in karma_views.iter().enumerate() {
                            @let last_row = row_i == karma_views.len() - 1;
                            tr {
                                td class=(if last_row { "bottom-left" } else { "" }) {
                                    (karma_view.karma_id)
                                }

                                (datastar_edit_cell("karma", karma_view.karma_id, "quantity", karma_view.karma_quantity.to_string(), None))
                                (datastar_edit_cell("karma", karma_view.karma_id, "name", karma_view.karma_name.clone(), None))
                                (datastar_edit_cell("karma_condition", karma_view.karma_condition_id, "quantity", karma_view.karma_condition_quantity.to_string(), None))
                                (datastar_edit_cell("karma", karma_view.karma_id, "condition_id", karma_view.karma_condition_id.to_string(), Some("condition")))

                                (datastar_edit_cell("karma_condition", karma_view.karma_condition_id, "name", karma_view.karma_condition_name.clone(), Some("condition")))

                                td {
                                    .karma-cell {
                                        .karma-primary.column {
                                            .div { (karma_view.karma_condition_explanation) }
                                            .row.separa {
                                                (datastar_edit_cell("karma_condition", karma_view.karma_condition_id, "condition", karma_view.karma_condition_condition.clone(), Some("condition")))
                                                .div { (karma_view.karma_condition_value.clone().unwrap_or_default()) }
                                            }
                                        }
                                    }
                                }

                                (datastar_edit_cell("karma", karma_view.karma_id, "operator", karma_view.karma_operator.clone(), None))

                                td {
                                    .karma-cell {
                                        .karma-primary.column {
                                            .div { (karma_view.karma_consequence_explanation) }
                                            .row.separa {
                                                (datastar_edit_cell("karma_consequence", karma_view.karma_consequence_id, "consequence", karma_view.karma_consequence_consequence.clone(), Some("consequence")))
                                                .div { (karma_view.karma_consequence_value.clone().unwrap_or_default()) }
                                            }
                                        }
                                    }
                                }

                                (datastar_edit_cell("karma_consequence", karma_view.karma_consequence_id, "name", karma_view.karma_consequence_name.clone(), Some("consequence")))
                                (datastar_edit_cell("karma", karma_view.karma_id, "consequence_id", karma_view.karma_consequence_id.to_string(), Some("consequence")))
                                (datastar_edit_cell("karma_consequence", karma_view.karma_consequence_id, "quantity", karma_view.karma_consequence_quantity.to_string(), Some("consequence")))
                            }
                        }
                    }
                }
            }
        },
        Err(e) => {
            log!(e, "Failed to create Karma View: {e}");
            html! { "Karma View is not available" }
        }
    }
}

async fn presentation_html_karma_view_condition(services: InjectedServices) -> Markup {
    match services.repository.karma.get_condition_view().await {
        Ok(conditions) => html! {
            table class="rounded-table" {
                thead {
                    tr {
                        th { "Quantity" }
                        th { "Id" }
                        th { "Name" }
                        th { "Condition" }
                    }
                }
                tbody {
                    @for (i, condition) in conditions.iter().enumerate() {
                        @let last_row = i == conditions.len() - 1;
                        tr {
                            (datastar_edit_cell("karma_condition", condition.id, "quantity", condition.quantity.to_string(), Some("condition")))
                            td { (condition.id) }
                            (datastar_edit_cell("karma_condition", condition.id, "name", condition.name.clone(), Some("condition")))
                            td class=(if last_row { "bottom-right" } else { "" }) {
                                div { (condition.explanation) }
                                .row.separa {
                                    (datastar_edit_cell("karma_condition", condition.id, "condition", condition.condition.clone(), Some("condition")))
                                    .div { (condition.value.clone().unwrap_or_default()) }
                                }
                            }
                        }
                    }
                }
            }
        },
        Err(e) => {
            log!(e, "Failed to create Karma Condition View: {e}");
            html! { "Karma Condition View is not available" }
        }
    }
}

async fn presentation_html_karma_view_consequence(services: InjectedServices) -> Markup {
    match services.repository.karma.get_consequence_view().await {
        Ok(consequences) => html! {
            table class="rounded-table" {
                thead {
                    tr {
                        th { "Consequence" }
                        th { "Name" }
                        th { "Id" }
                        th { "Quantity" }
                    }
                }
                tbody {
                    @for (i, consequence) in consequences.iter().enumerate() {
                        @let _last_row = i == consequences.len() - 1;
                        tr {
                            td.column {
                                div { (consequence.explanation) }
                                .row.separa {
                                    (datastar_edit_cell("karma_consequence", consequence.id, "consequence", consequence.consequence.clone(), Some("consequence")))
                                    .div { (consequence.value.clone().unwrap_or_default()) }
                                }
                            }
                            (datastar_edit_cell("karma_consequence", consequence.id, "name", consequence.name.clone(), Some("consequence")))
                            td { (consequence.id) }
                            (datastar_edit_cell("karma_consequence", consequence.id, "quantity", consequence.quantity.to_string(), Some("consequence")))
                        }
                    }
                }
            }
        },
        Err(e) => {
            log!(e, "Failed to create Karma Consequence View: {e}");
            html! { "Karma Consequence View is not available" }
        }
    }
}

fn datastar_edit_cell(
    table: &str,
    id: u32,
    column: &str,
    value: String,
    search: Option<&str>,
) -> Markup {
    let id_attr = cell_id(table, id, column);
    html! {
        td id=(id_attr) data-signals="{editing: false}" {
            div data-show="!$editing" data-on:click="$editing = true" {
                button type="button" class="plain-button" {
                    (value.clone())
                }
            }
            form
                data-show="$editing"
                data-on:submit__prevent=(format!(
                    "@patch('/table/{}/{}/{}', {{contentType: 'form'}})",
                    table, id, column
                ))
            {
                @if let Some(search) = search {
                    input type="hidden" name="search" value=(search) {}
                }
                input name="value" value=(value) {}
                button type="submit" { "Save" }
                button type="button" data-on:click="$editing = false" { "Cancel" }
            }
        }
    }
}
