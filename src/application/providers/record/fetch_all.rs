use crate::infrastructure::database::repositories::record::fetch_all;

pub async fn fetch_all() {
    let records = fetch_all().await;
}
