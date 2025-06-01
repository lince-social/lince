use crate::infrastructure::database::repositories::view::repository_view_toggle_view_id;
use std::io::Error;

pub async fn provider_view_toggle_view_id(
    collection_id: String,
    view_id: String,
) -> Result<(), Error> {
    repository_view_toggle_view_id(view_id, collection_id).await?;
    Ok(())
}
