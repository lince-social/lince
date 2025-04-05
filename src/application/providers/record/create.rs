use std::io::Error;

use crate::{
    domain::entities::record::RecordSchemaCreate,
    infrastructure::database::repositories::record::repository_record_create,
};

pub async fn provider_record_create(record: RecordSchemaCreate) -> Result<(), Error> {
    repository_record_create(record).await
}
