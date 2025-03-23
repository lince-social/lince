use super::schema::schema;
use super::seed::seed;

pub async fn tidy_database() {
    let _schema = schema().await;
    let _seed = seed().await;
}
