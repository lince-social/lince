use crate::infrastructure::database::repositories::query::repository_query_execute;
use std::io::Error;

pub async fn provider_query_execute(query: String) -> Result<(), Error> {
    repository_query_execute(query).await
}
