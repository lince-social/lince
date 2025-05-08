use crate::infrastructure::database::repositories::record::repository_record_set_quantity;
use std::io::Error;

pub async fn provider_record_set_quantity(id: u32, quantity: f64) -> Result<(), Error> {
    repository_record_set_quantity(id, quantity).await
}

pub fn provider_record_set_quantity_sync(id: u32, quantity: f64) -> Result<(), Error> {
    futures::executor::block_on(async { repository_record_set_quantity(id, quantity).await })
}
