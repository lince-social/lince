use maud::{Markup, html};

pub async fn presentation_web_view_toggle_all(collection_id: u32) -> Markup {
    html! {
       button hx-patch=(format!("/view/toggle/{}", collection_id)) hx-target="#body" { "T"}
    }
}
