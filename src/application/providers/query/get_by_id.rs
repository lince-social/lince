use crate::{
    domain::entities::operation::Query,
    infrastructure::database::repositories::query::repository_query_get_by_id,
};
use std::io::Error;

pub async fn provider_query_get_by_id(id: u32) -> Result<Query, Error> {
    repository_query_get_by_id(id).await
}
