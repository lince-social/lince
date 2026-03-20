use injection::cross_cutting::InjectedServices;
use std::io::Error;

pub async fn query_execute(services: InjectedServices, id: u32) -> Result<(), Error> {
    let sql = services.repository.query.get_by_id(id).await?;
    crate::write::execute_sql(services, sql.query)
        .await
        .map(|_| ())
}
