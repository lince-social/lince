use crate::{
    domain::entities::table::Table,
    infrastructure::database::repositories::karma::repository_karma_condition,
};
use std::io::Error;

pub async fn provider_karma_get_condition() -> Result<Vec<(String, Table)>, Error> {
    repository_karma_condition().await
}
