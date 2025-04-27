use crate::infrastructure::database::repositories::record::repository_record_get_quantity_by_id;
use std::io::Error;

pub fn provider_record_get_quantity_by_id_sync(id: u32) -> Result<f64, Error> {
    futures::executor::block_on(async { repository_record_get_quantity_by_id(id).await })
}
