use crate::{
    application::schema::selection::row::ConfigurationRow,
    infrastructure::database::repositories::selection::repository_selection_get_active,
};

pub async fn provider_selection_get_active() -> ConfigurationRow {
    let (selection, queried_views) = repository_selection_get_active().await.unwrap();
    (selection, queried_views)
}
