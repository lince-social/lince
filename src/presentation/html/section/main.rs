use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::{
        pages::karma::orchestra::presentation_html_karma_orchestra,
        table::tables::presentation_html_tables,
    },
};

pub async fn presentation_html_section_main(services: InjectedServices) -> String {
    let (tables, special_views) = services
        .providers
        .view
        .get_active_view_data()
        .await
        .unwrap();

    let mut content = presentation_html_tables(tables).await.0;

    for special_view in special_views {
        content.push_str(&match special_view.as_str() {
            "karma_orchestra" => presentation_html_karma_orchestra(services.clone()).await,
            _ => String::new(), // fallback to empty string
        });
    }

    content
}
