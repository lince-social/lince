use std::io::Error;

use crate::{
    domain::entities::record::Record,
    infrastructure::database::repositories::record::record_repository_fetch_all,
};

pub async fn record_providers_fetch_all() -> Result<Vec<Record>, Error> {
    record_repository_fetch_all().await
}
