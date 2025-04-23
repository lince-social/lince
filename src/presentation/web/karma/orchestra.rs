use crate::presentation::web::section::body::nav_body;
use maud::html;

pub async fn presentation_web_karma_orchestra() -> String {
    let element = html!("").0;
    nav_body(element).await
}
