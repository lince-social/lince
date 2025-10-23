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
        Ok(karma_views) => {
            html! {
                main id="main" {
                    div {
                        p { "Karma View" }
                        table class="rounded-table" {
                            thead {
                                tr {
                                    th.karma-view-karma  class="top-left" { "id" }
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
                                    th.karma-view-condition.top-right  { "Kcs Qty" }
                                }
                            }
                            tbody {
                                @for (row_i, karma_view) in karma_views.iter().enumerate() {
                                    @let last_row = row_i == karma_views.len() - 1;
                                    tr {
                                        td class=(if last_row { "bottom-left" } else { "" }) {
                                            (karma_view.karma_id)
                                        }
                                        td { (karma_view.karma_quantity) }
                                        td { (karma_view.karma_name) }

                                        td { (karma_view.karma_condition_quantity) }
                                        td { (karma_view.karma_condition_id) }
                                        td { (karma_view.karma_condition_name) }
                                        td {
                                            .karma-cell {
                                                .karma-primary.column {
                                                    .div {(karma_view.karma_condition_explanation)}
                                                    .row {
                                                        .div { (karma_view.karma_condition_condition) }
                                                        .div{ (karma_view.karma_condition_value.clone().unwrap_or_default()) }
                                                    }
                                                }
                                            }
                                        }

                                        td { (karma_view.karma_operator) }

                                        td {
                                            .karma-cell {
                                                .karma-primary.column {
                                                    .div {(karma_view.karma_consequence_explanation)}
                                                    .row.fence--row.separa {
                                                        .div { (karma_view.karma_consequence_consequence) }
                                                        .div {(karma_view.karma_consequence_value.clone().unwrap_or_default())}
                                                    }
                                                }
                                            }
                                        }
                                        td { (karma_view.karma_consequence_name) }
                                        td { (karma_view.karma_consequence_id) }
                                        td class=(if last_row { "bottom-right" } else { "" }) { (karma_view.karma_consequence_quantity) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            log!(e, "Failed to create Karma View: {e}");
            html!({ "Karma View is not available" })
        }
    }
}

async fn presentation_html_karma_view_condition(services: InjectedServices) -> Markup {
    match services.repository.karma.get_condition_view().await {
        Ok(conditions) => {
            html! {
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
                                td  { (condition.quantity) }
                                td { (condition.id) }
                                td { (condition.name) }
                                td class=(if last_row { "bottom-right" } else { "" }){
                                    div { (condition.explanation) }
                                    .row.separa {
                                        div { (condition.condition) }
                                        div { (condition.value.clone().unwrap_or_default()) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            log!(e, "Failed to create Karma Condition View: {e}");
            html!({ "Karma Condition View is not available" })
        }
    }
}
async fn presentation_html_karma_view_consequence(services: InjectedServices) -> Markup {
    match services.repository.karma.get_consequence_view().await {
        Ok(consequences) => {
            html! {
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
                                        div { (consequence.consequence) }
                                        div { (consequence.value.clone().unwrap_or_default()) }
                                    }
                                }
                                td { (consequence.name) }
                                td { (consequence.id) }
                                td class=(if last_row { "bottom-right" } else { "" }) { (consequence.quantity) }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            log!(e, "Failed to create Karma Consequence View: {e}");
            html!({ "Karma Consequence View is not available" })
        }
    }
}
