use crate::application::providers::frequency::get::provider_frequency_get;

pub async fn use_case_frequency_get_name(id: u32) -> Option<String> {
    match provider_frequency_get(id).await {
        None => None,
        Some(frequency) => Some(frequency.name),
    }
}
