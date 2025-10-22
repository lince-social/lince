use crate::{infrastructure::cross_cutting::InjectedServices, log};
use maud::html;

pub async fn presentation_html_karma_view(services: InjectedServices) -> String {
    match services.repository.karma.get_view().await {
        Ok(karma_views) => {
            html! {
                main id="main" {
                    div {
                        p { "Karma View" }
                        table class="rounded-table" {
                            thead {
                                tr {
                                    th class="top-left" { "id" }
                                    th { "Name" }
                                    th { "Qty" }
                                    th { "Kcs Cs" }
                                    th { "Kcd Name" }
                                    th { "Kcd Qty" }
                                    th { "Kcd Id" }
                                    th { "Operand" }
                                    th { "Kcs Id" }
                                    th { "Kcs Qty" }
                                    th { "Kcs Name" }
                                    th class="top-right" { "Kcs Cs" }
                                }
                            }
                            tbody {
                                @for (row_i, karma_view) in karma_views.iter().enumerate() {
                                    @let last_row = row_i == karma_views.len() - 1;
                                    tr {
                                        td class=(if last_row { "bottom-left" } else { "" }) {
                                            (karma_view.karma_id)
                                        }
                                        td { (karma_view.karma_name) }
                                        td { (karma_view.karma_quantity) }

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

                                        td { (karma_view.karma_condition_name) }
                                        td { (karma_view.karma_condition_quantity) }
                                        td { (karma_view.karma_condition_id) }
                                        td { (karma_view.karma_operator) }
                                        td { (karma_view.karma_consequence_id) }
                                        td { (karma_view.karma_consequence_quantity) }
                                        td { (karma_view.karma_consequence_name) }

                                        // Karma Consequence Cell - shows consequence
                                        td class=(if last_row { "bottom-right" } else { "" }) {
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
    .0
}
