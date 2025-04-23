use crate::{
    domain::entities::table::Table,
    infrastructure::database::repositories::view::repository_view_get_active_view_data,
};

pub async fn provider_view_get_active_view_data() -> Result<Vec<(String, Table)>, sqlx::Error> {
    repository_view_get_active_view_data().await
}
