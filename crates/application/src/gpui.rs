use domain::dirty::gpui::State;
use injection::cross_cutting::InjectedServices;
use std::io::Error;

pub async fn get_gpui_startup_data(services: InjectedServices) -> Result<State, Error> {
    let (tables, special_views) = services
        .repository
        .collection
        .get_active_view_data()
        .await?;
    Ok(State {
        collections: services.repository.collection.get_all().await?,
        tables,
        special_views,
        collection_view_column_widths: services
            .repository
            .collection
            .get_all_collection_view_column_widths()
            .await?,
    })
}
