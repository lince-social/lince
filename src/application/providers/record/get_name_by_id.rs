use crate::infrastructure::database::repositories::record::repository_record_get_head_by_id;
use std::io::Error;

pub async fn provider_record_get_head_by_id(id: u32) -> Result<String, Error> {
    repository_record_get_head_by_id(id).await
}
