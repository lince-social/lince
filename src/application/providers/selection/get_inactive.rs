use crate::{
    application::schema::selection::row::ConfigurationRow,
    infrastructure::database::repositories::selection::repository_selection_get_inactive,
};

pub async fn provider_selection_get_inactive() -> Vec<ConfigurationRow> {
    repository_selection_get_inactive().await.unwrap()
}
