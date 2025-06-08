use crate::{
    domain::entities::frequency::Frequency,
    infrastructure::cross_cutting::InjectedServices,
};

pub async fn use_case_frequency_update(services: InjectedServices, frequencies_to_update: Vec<Frequency>) {
    for frequency in frequencies_to_update {
        services.providers.frequency.update(frequency).await;
    }
}
