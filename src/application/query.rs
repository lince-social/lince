use crate::infrastructure::cross_cutting::InjectedServices;
use std::io::Error;

pub async fn query_execute(services: InjectedServices, id: u32) -> Result<(), Error> {
    let sql = services.repository.query.get_by_id(id).await?;
    services.repository.query.execute(&sql.query).await
}
