use axum::response::Html;

use crate::presentation::web::karma::orchestra::presentation_web_karma_orchestra;

pub async fn handler_get_karma_orchestra() -> Html<String> {
    Html(presentation_web_karma_orchestra().await)
}
