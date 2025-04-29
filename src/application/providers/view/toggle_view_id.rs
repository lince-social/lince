use crate::infrastructure::database::repositories::view::repository_view_toggle_view_id;
use std::io::Error;

pub async fn provider_view_toggle_view_id(id: String) -> Result<(), Error> {
    let res = repository_view_toggle_view_id(id).await;
    if res.is_err() {
        println!("{}", res.unwrap_err());
    }
    Ok(())
}
