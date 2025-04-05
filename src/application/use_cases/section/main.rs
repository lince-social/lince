use std::io::Error;

use crate::{
    application::providers::record::fetch_all::record_providers_fetch_all,
    domain::entities::record::Record,
};

pub async fn main_use_case() -> Result<Vec<Record>, Error> {
    record_providers_fetch_all().await
}
