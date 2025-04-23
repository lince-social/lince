use axum::Router;

use super::karma::router_karma;

pub async fn router_page() -> Router {
    Router::new().nest("/karma_orchestra", router_karma().await)
}
