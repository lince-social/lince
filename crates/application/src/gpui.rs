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
        pinned_views: services.repository.collection.get_pinned_views().await?,
        pinned_tables: services
            .repository
            .collection
            .get_pinned_view_data()
            .await?,
        views_with_pin_info: services
            .repository
            .collection
            .get_views_with_pin_info()
            .await?,
    })
}
