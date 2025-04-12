use std::io::Error;

use crate::infrastructure::database::repositories::view::repository_view_toggle;

pub async fn provider_view_toggle(id: String) -> Result<(), Error> {
    let res = repository_view_toggle(id).await;
    if res.is_err() {
        println!("{}", res.unwrap_err());
    }
    Ok(())
}
