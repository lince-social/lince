use crate::infrastructure::{
    cross_cutting::InjectedServices, http::handlers::collection::handler_collection_set_active,
};
use axum::{Router, routing::patch};

pub fn collection_router(services: InjectedServices) -> Router {
    Router::new()
        .route("/active/{id}", patch(handler_collection_set_active))
        .with_state(services)
}
