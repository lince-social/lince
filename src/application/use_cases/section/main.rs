use crate::{
    application::providers::record::fetch_all::fetch_all, domain::entities::record::Record,
};

pub async fn main_use_case() -> Option<Vec<Record>> {
    let records = fetch_all().await;
}
