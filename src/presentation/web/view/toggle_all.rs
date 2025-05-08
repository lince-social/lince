use maud::{Markup, html};

pub async fn presentation_web_view_toggle_all(configuration_id: u32) -> Markup {
    html! {
       button hx-patch=(format!("/view/toggle/{}", configuration_id)) hx-target="#body" { "T"}
    }
}
