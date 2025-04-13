use crate::{
    domain::entities::frequency::Frequency,
    infrastructure::database::repositories::frequency::repository_frequency_update,
};

pub async fn provider_frequency_update(frequency: Frequency) {
    repository_frequency_update(frequency).await
}
