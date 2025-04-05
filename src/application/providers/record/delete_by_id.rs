use std::io::Error;

use crate::infrastructure::database::repositories::record::repository_record_delete_by_id;

pub async fn provider_record_delete_by_id(id: String) -> Result<(), Error> {
    repository_record_delete_by_id(id).await
}
