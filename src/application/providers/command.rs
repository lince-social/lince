// use crate::domain::{entities::command::Command, repositories::command::CommandRepository};
// use std::io::Error;

// pub struct CommandProvider {
//     pub repository: std::sync::Arc<dyn CommandRepository>,
// }

// impl CommandProvider {
//     pub async fn get_by_id(&self, id: u32) -> Result<Option<Command>, Error> {
//         self.repository.get_by_id(id).await
//     }
// }
