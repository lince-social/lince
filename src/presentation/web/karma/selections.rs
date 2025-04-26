use crate::{
    application::providers::karma::{
        get_condition::provider_karma_get_condition,
        get_consequence::provider_karma_get_consequence, get_joined::provider_karma_get_joined,
    },
    presentation::web::table::tables::presentation_web_tables,
};
use maud::{Markup, html};

pub async fn presentation_web_karma_condition(page: String) -> Markup {
    match provider_karma_get_condition().await {
        Ok(table) => presentation_web_tables(page, table).await,
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}

pub async fn presentation_web_karma_consequence(page: String) -> Markup {
    match provider_karma_get_consequence().await {
        Ok(table) => presentation_web_tables(page, table).await,
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}

pub async fn presentation_web_karma_karma(page: String) -> Markup {
    match provider_karma_get_joined().await {
        Ok(table) => presentation_web_tables(page, table).await,
        Err(_) => html!({ "Karma Condition is not available" }),
    }
}
