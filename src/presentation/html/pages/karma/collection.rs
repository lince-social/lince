use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::{
        table::tables::presentation_html_tables_karma, utils::to_table::to_named_sorted_table,
    },
};
use maud::{Markup, html};

pub async fn presentation_html_karma_condition(services: InjectedServices) -> Markup {
    match services.repository.karma.get_condition().await {
        Ok(table) => {
            presentation_html_tables_karma(services, to_named_sorted_table("condition", table))
                .await
        }
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}

pub async fn presentation_html_karma_consequence(services: InjectedServices) -> Markup {
    match services.repository.karma.get_consequence().await {
        Ok(table) => {
            presentation_html_tables_karma(services, to_named_sorted_table("consequence", table))
                .await
        }
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}

pub async fn presentation_html_karma_karma(services: InjectedServices) -> Markup {
    match services.repository.karma.get(None).await {
        Ok(table) => {
            presentation_html_tables_karma(services, to_named_sorted_table("karma", table)).await
        }
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}
