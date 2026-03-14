use maud::{Markup, html};

pub async fn presentation_html_view_toggle_all(collection_id: u32) -> Markup {
    html! {
       button #button-collection-id type="button" data-on:click=(format!("@patch('/view/toggle/{}')", collection_id)) { "T"}
    }
}
