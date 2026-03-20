use injection::cross_cutting::InjectedServices;
use persistence::repositories::view::ViewSnapshot;
use std::io::Error;

pub async fn view_snapshot(
    services: InjectedServices,
    view_id: u32,
) -> Result<ViewSnapshot, Error> {
    services.repository.view.read_snapshot(view_id).await
}

pub async fn view_dependencies(
    services: InjectedServices,
    view_id: u32,
) -> Result<std::collections::BTreeSet<String>, Error> {
    services.repository.view.get_dependencies(view_id).await
}
