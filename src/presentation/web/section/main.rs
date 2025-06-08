use futures::future::join_all;

use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::{
        pages::karma::orchestra::presentation_web_karma_orchestra,
        table::tables::presentation_web_tables,
    },
};

pub async fn presentation_web_section_main(services: InjectedServices) -> String {
    let (tables, special_views) = services.providers.view.get_active_view_data().await.unwrap();
    let mut content = presentation_web_tables(tables).await.0;

    let special_futures = special_views.iter().map(|special_view| async move {
        match special_view.as_str() {
            "karma_orchestra" => presentation_web_karma_orchestra(services.clone()).await,
            _ => String::new(), // fallback to empty string
        }
    });

    let results = join_all(special_futures).await;

    for result in results {
        content.push_str(&result);
    }

    content
}
