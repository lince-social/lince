use maud::{Markup, html};

use crate::application::providers::record::fetch_all::record_providers_fetch_all;

pub async fn get_records_component() -> Markup {
    let records = record_providers_fetch_all().await;
    let records = records.unwrap();

    html! {
    div id="main" {
            @for record in &records {
                div {
                    p { (record.head) }

                    button
                    hx-delete=(format!("/record/{}", record.id))
                        hx-swap="outerHTML"
                        hx-target="#main"
                    hx-trigger="click"
                    { "x" }
                }
            }
        }
    }
}
