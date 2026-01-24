use domain::dirty::gpui::State;
use injection::cross_cutting::InjectedServices;
use std::io::Error;

pub async fn get_gpui_startup_data(services: InjectedServices) -> Result<State, Error> {
    Ok(State {
        collections: services.repository.collection.get_all().await?,
        tables: services
            .repository
            .collection
            .get_active_view_data()
            .await?
            .0,
    })
}
