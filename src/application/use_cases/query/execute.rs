use crate::infrastructure::cross_cutting::InjectedServices;
use std::io::Error;

pub async fn use_case_query_execute(services: InjectedServices, id: u32) -> Result<(), Error> {
    let sql = services.providers.query.get_by_id(id).await?;
    services.providers.query.execute(&sql.query).await
}
