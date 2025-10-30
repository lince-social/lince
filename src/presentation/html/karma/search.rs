use maud::html;

use crate::{infrastructure::cross_cutting::InjectedServices, log};

pub async fn presentation_html_karma_get_condition(
    services: InjectedServices,
    search: Option<String>,
) -> String {
    match services.repository.karma.get_condition_tokens(search).await {
        Ok(tokens) => {
            html! {
                .northeast_modal.filled id="karma-search-modal" {
                @for (condition_id, _, condition_explanation, condition_value) in tokens {
                    .row.s_gap.m_padding {
                        div { (condition_id) }
                        div {" |"}
                        div { (condition_value) }
                        div {" |"}
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
