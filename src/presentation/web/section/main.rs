use futures::future::join_all;

use crate::{
    application::providers::view::get_active_view_data::provider_view_get_active_view_data,
    presentation::web::{
        karma::orchestra::presentation_web_karma_orchestra, table::tables::presentation_web_tables,
    },
};

pub async fn presentation_web_section_main() -> String {
    let (tables, special_views) = provider_view_get_active_view_data().await.unwrap();
    let mut content = presentation_web_tables(tables).await.0;

    let special_futures = special_views.iter().map(|special_view| async move {
        match special_view.as_str() {
            "karma_orchestra" => presentation_web_karma_orchestra().await,
            _ => String::new(), // fallback to empty string
        }
    });

    let results = join_all(special_futures).await;

    for result in results {
        content.push_str(&result);
    }

    content
}
