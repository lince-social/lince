use crate::infrastructure::database::repositories::command::repository_command_get_by_id;
use std::io::Error;

pub async fn provider_karma_get_command_by_id(id: u32) -> Result<String, Error> {
    repository_command_get_by_id(id).await
}
