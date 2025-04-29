use crate::{
    domain::entities::table::Table,
    infrastructure::database::repositories::karma::repository_karma_get_joined,
};
use std::io::Error;

pub async fn provider_karma_get_joined() -> Result<Vec<(String, Table)>, Error> {
    repository_karma_get_joined().await
}
