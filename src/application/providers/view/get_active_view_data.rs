use std::collections::HashMap;

use crate::infrastructure::database::repositories::view::repository_view_get_active_view_data;

pub async fn provider_view_get_active_view_data()
-> Result<Vec<(String, Vec<HashMap<String, String>>)>, sqlx::Error> {
    repository_view_get_active_view_data().await
}
