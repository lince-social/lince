use crate::htmx_edit_cell;
use crate::{infrastructure::cross_cutting::InjectedServices, log};
use maud::{Markup, html};

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
            main id="main" {
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

                                    (htmx_edit_cell!("karma", karma_view.karma_id, "quantity", karma_view.karma_quantity))
                                    (htmx_edit_cell!("karma", karma_view.karma_id, "name", karma_view.karma_name))
                                    (htmx_edit_cell!("karma_condition", karma_view.karma_condition_id, "quantity", karma_view.karma_condition_quantity))
                                    td { (karma_view.karma_condition_id) }
                                    (htmx_edit_cell!("karma_condition", karma_view.karma_condition_id, "name", karma_view.karma_condition_name))

                                    td {
                                        .karma-cell {
                                            .karma-primary.column {
                                                .div { (karma_view.karma_condition_explanation) }
                                                .row.separa {
                                                    (htmx_edit_cell!("karma_condition", karma_view.karma_condition_id, "condition", karma_view.karma_condition_condition, div))
                                                    .div { (karma_view.karma_condition_value.clone().unwrap_or_default()) }
                                                }
                                            }
                                        }
                                    }

                                    (htmx_edit_cell!("karma", karma_view.karma_id, "operator", karma_view.karma_operator))

                                    td {
                                        .karma-cell {
                                            .karma-primary.column {
                                                .div { (karma_view.karma_consequence_explanation) }
                                                .row.separa {
                                                    (htmx_edit_cell!("karma_consequence", karma_view.karma_consequence_id, "consequence", karma_view.karma_consequence_consequence, div))
                                                    .div { (karma_view.karma_consequence_value.clone().unwrap_or_default()) }
                                                }
                                            }
                                        }
                                    }

                                    (htmx_edit_cell!("karma_consequence", karma_view.karma_consequence_id, "name", karma_view.karma_consequence_name))
                                    (htmx_edit_cell!("karma", karma_view.karma_id, "consequence_id", karma_view.karma_consequence_id))
                                    (htmx_edit_cell!("karma_consequence", karma_view.karma_consequence_id, "quantity", karma_view.karma_consequence_quantity))
                                }
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
                            (htmx_edit_cell!("karma_condition", condition.id, "quantity", condition.quantity))
                            td { (condition.id) }
                            (htmx_edit_cell!("karma_condition", condition.id, "name", condition.name))
                            td class=(if last_row { "bottom-right" } else { "" }) {
                                div { (condition.explanation) }
                                .row.separa {
                                    (htmx_edit_cell!("karma_condition", condition.id, "condition", condition.condition, div))
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
                        @let last_row = i == consequences.len() - 1;
                        tr {
                            td.column {
                                div { (consequence.explanation) }
                                .row.separa {
                                    (htmx_edit_cell!("karma_consequence", consequence.id, "consequence", consequence.consequence, div))
                                    .div { (consequence.value.clone().unwrap_or_default()) }
                                }
                            }
                            (htmx_edit_cell!("karma_consequence", consequence.id, "name", consequence.name))
                            td { (consequence.id) }
                            (htmx_edit_cell!("karma_consequence", consequence.id, "quantity", consequence.quantity))
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
