use crate::application::{
    providers::collection::get_inactive::provider_collection_get_inactive,
    schema::{collection::row::ConfigurationForBarScheme, view::queried_view::QueriedView},
};

pub async fn use_case_collection_get_inactive() -> Vec<(ConfigurationForBarScheme, Vec<QueriedView>)>
{
    provider_collection_get_inactive().await
}
