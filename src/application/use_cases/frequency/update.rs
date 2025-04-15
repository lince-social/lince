use crate::{
    application::providers::frequency::update::provider_frequency_update,
    domain::entities::frequency::Frequency,
};

pub fn use_case_frequency_update(frequencies_to_update: Vec<Frequency>) {
    futures::executor::block_on(async {
        for frequency in frequencies_to_update {
            provider_frequency_update(frequency).await
        }
    })
}
