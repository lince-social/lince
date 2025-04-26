use crate::{
    domain::entities::table::Table,
    infrastructure::database::repositories::karma::repository_karma_consequence,
};
use std::io::Error;

pub async fn provider_karma_get_consequence() -> Result<Vec<(String, Table)>, Error> {
    repository_karma_consequence().await
}
