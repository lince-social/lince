use crate::infrastructure::cross_cutting::InjectedServices;

pub async fn use_case_frequency_get_name(services: InjectedServices, id: String) -> String {
    match services.providers.frequency.get(id).await {
        Some(frequency) => frequency.name,
        None => String::new(),
    }
}
