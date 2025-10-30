use maud::{Markup, html};

pub async fn presentation_html_view_toggle_all(collection_id: u32) -> Markup {
    html! {
       button #button-collection-id hx-patch=(format!("/view/toggle/{}", collection_id)) hx-target="#body" { "T"}
    }
}
