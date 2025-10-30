use crate::infrastructure::{
    cross_cutting::InjectedServices,
    http::handlers::karma::{handler_karma_get_condition, handler_karma_post_condition},
};
use axum::{Router, routing::get};

pub fn karma_router(services: InjectedServices) -> Router {
    Router::new()
        .route(
            "/condition",
            get(handler_karma_get_condition).post(handler_karma_post_condition),
        )
        .with_state(services)
}
