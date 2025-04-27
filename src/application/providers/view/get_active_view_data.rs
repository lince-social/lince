use crate::{
    domain::entities::table::Table,
    infrastructure::database::repositories::view::repository_view_get_active_view_data,
};
use std::io::Error;

pub async fn provider_view_get_active_view_data()
-> Result<(Vec<(String, Table)>, Vec<String>), Error> {
    repository_view_get_active_view_data().await
}
