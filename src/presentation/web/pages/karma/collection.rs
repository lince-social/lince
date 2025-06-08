use crate::infrastructure::cross_cutting::InjectedServices;
use maud::{html, Markup};

pub async fn presentation_web_karma_condition(services: InjectedServices) -> Markup {
    match services.providers.karma.get_condition().await {
        Ok(Some(condition)) => html! {
            div class="condition" {
                (condition)
            }
        },
        _ => html! {
            div class="condition" {
                "No condition"
            }
        },
    }
}

pub async fn presentation_web_karma_consequence(services: InjectedServices) -> Markup {
    match services.providers.karma.get_consequence().await {
        Ok(Some(consequence)) => html! {
            div class="consequence" {
                (consequence)
            }
        },
        _ => html! {
            div class="consequence" {
                "No consequence"
            }
        },
    }
}

pub async fn presentation_web_karma_karma(services: InjectedServices) -> Markup {
    match services.providers.karma.get_joined().await {
        Ok(Some(karma)) => html! {
            div class="karma" {
                (karma)
            }
        },
        _ => html! {
            div class="karma" {
                "No karma"
            }
        },
    }
}
