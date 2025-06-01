use crate::application::{
    providers::selection::get_inactive::provider_selection_get_inactive,
    schema::{selection::row::ConfigurationForBarScheme, view::queried_view::QueriedView},
};

pub async fn use_case_selection_get_inactive() -> Vec<(ConfigurationForBarScheme, Vec<QueriedView>)>
{
    provider_selection_get_inactive().await
}
