use crate::{
    domain::entities::frequency::Frequency,
    infrastructure::database::repositories::frequency::repository_frequency_get,
};

pub async fn provider_frequency_get(id: u32) -> Option<Frequency> {
    repository_frequency_get(id).await
}
