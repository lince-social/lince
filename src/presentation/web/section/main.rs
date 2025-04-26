use maud::Markup;

use crate::{
    application::providers::view::get_active_view_data::provider_view_get_active_view_data,
    presentation::web::table::tables::presentation_web_tables,
};

pub async fn presentation_web_section_main() -> Markup {
    let page = "main".to_string();
    let tables = provider_view_get_active_view_data().await.unwrap();
    presentation_web_tables(page, tables).await
}
