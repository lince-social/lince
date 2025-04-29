use crate::infrastructure::database::repositories::record::repository_record_set_quantity;

pub async fn provider_record_set_quantity(id: String, quantity: f64) {
    repository_record_set_quantity(id, quantity).await
}

pub fn provider_record_set_quantity_sync(id: String, quantity: f64) {
    futures::executor::block_on(async { repository_record_set_quantity(id, quantity).await })
}
