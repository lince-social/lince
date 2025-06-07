use crate::infrastructure::{
    cross_cutting::InjectedServices,
    http::handlers::view::{handler_view_toggle_collection_id, handler_view_toggle_view_id},
};
use axum::{Router, routing::patch};

pub fn view_router(services: InjectedServices) -> Router {
    Router::new()
        .route(
            "/toggle/{collection_id}/{view_id}",
            patch(handler_view_toggle_view_id),
        )
        .route(
            "/toggle/collection/{collection_id}",
            patch(handler_view_toggle_collection_id),
        )
        .with_state(services)
}
