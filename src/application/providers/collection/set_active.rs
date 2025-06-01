use crate::infrastructure::database::repositories::collection::repository_collection_set_active;

pub async fn provider_collection_set_active(id: String) {
    repository_collection_set_active(id).await
}
