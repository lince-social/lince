use crate::infrastructure::database::repositories::view::repository_view_toggle_selection_id;
use std::io::Error;

pub async fn provider_view_toggle_selection_id(id: String) -> Result<(), Error> {
    repository_view_toggle_selection_id(id).await?;
    Ok(())
}
