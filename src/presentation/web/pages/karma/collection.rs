use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::{
        table::tables::presentation_web_tables_karma, utils::to_table::to_named_sorted_table,
    },
};
use maud::{Markup, html};

pub async fn presentation_web_karma_condition(services: InjectedServices) -> Markup {
    match services.providers.karma.get_condition().await {
        Ok(table) => {
            presentation_web_tables_karma(services, to_named_sorted_table("condition", table)).await
        }
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}

pub async fn presentation_web_karma_consequence(services: InjectedServices) -> Markup {
    match services.providers.karma.get_consequence().await {
        Ok(table) => {
            presentation_web_tables_karma(services, to_named_sorted_table("consequence", table))
                .await
        }
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}

pub async fn presentation_web_karma_karma(services: InjectedServices) -> Markup {
    match services.providers.karma.get().await {
        Ok(table) => {
            presentation_web_tables_karma(services, to_named_sorted_table("karma", table)).await
        }
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}
