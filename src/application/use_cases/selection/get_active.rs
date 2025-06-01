use crate::application::{
    providers::selection::get_active::provider_selection_get_active,
    schema::selection::row::ConfigurationRow,
};

pub async fn use_case_selection_get_active() -> ConfigurationRow {
    provider_selection_get_active().await
}
