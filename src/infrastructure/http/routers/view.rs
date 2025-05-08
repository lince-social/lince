use crate::infrastructure::http::handlers::view::{
    handler_view_toggle_configuration_id, handler_view_toggle_view_id,
};
use axum::{Router, routing::patch};

pub async fn view_router() -> Router {
    Router::new()
        .route(
            "/toggle/{configuration_id}/{view_id}",
            patch(handler_view_toggle_view_id),
        )
        .route(
            "/toggle/configuration/{configuration_id}",
            patch(handler_view_toggle_configuration_id),
        )
}
