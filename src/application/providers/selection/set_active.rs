use crate::infrastructure::database::repositories::selection::repository_selection_set_active;

pub async fn provider_selection_set_active(id: String) {
    repository_selection_set_active(id).await
}
