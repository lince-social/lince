use crate::infrastructure::database::repositories::view::repository_view_toggle_configuration_id;
use std::io::Error;

pub async fn provider_view_toggle_configuration_id(id: String) -> Result<(), Error> {
    repository_view_toggle_configuration_id(id).await?;
    Ok(())
}
