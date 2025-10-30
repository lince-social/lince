use maud::html;

use crate::{infrastructure::cross_cutting::InjectedServices, log};

pub async fn presentation_html_karma_get_condition(
    services: InjectedServices,
    search: Option<String>,
) -> String {
    match services.repository.karma.get_condition_tokens(search).await {
        Ok(tokens) => {
            dbg!(&tokens);
            html! {
                div id="karma-search-modal" {
                @for (condition_id, condition_value, condition_explanation) in tokens {
                    .row.s_gap {
                        div { (condition_id) }
                        div { (condition_value) }
                        div { (condition_explanation) }
                    }
                }
                }
            }
            .0
        }
        Err(e) => {
            log!(e, "Error getting conditions");
            "Error getting Conditions".to_string()
        }
    }
}
