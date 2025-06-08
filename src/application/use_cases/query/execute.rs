use std::io::Error;
use crate::infrastructure::cross_cutting::InjectedServices;

pub async fn use_case_query_execute(services: InjectedServices, id: u32) -> Result<(), Error> {
    let query = services.providers.query.get_by_id(id).await?;
    services.providers.query.execute(query).await
}
