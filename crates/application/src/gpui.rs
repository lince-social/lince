use injection::cross_cutting::InjectedServices;
use std::io::Error;

#[derive(Clone, Debug)]
pub struct GpuiStartupData {
    pub collections: Vec<String>,
    pub tables: Vec<String>,
}

pub async fn get_gpui_startup_data(services: InjectedServices) -> Result<GpuiStartupData, Error> {
    Ok(GpuiStartupData {
        collections: services
            .repository
            .collection
            .get_all()
            .await?
            .iter()
            .map(|_| "collection".to_string())
            .collect(),
        tables: services
            .repository
            .collection
            .get_active_view_data()
            .await?
            .0
            .iter()
            .map(|_| "table".to_string())
            .collect(),
    })
}
