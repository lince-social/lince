use crate::infrastructure::database::repositories::record::repository_record_zero_quantity;

pub async fn provider_record_zero_quantity(id: String) {
    repository_record_zero_quantity(id).await
}
