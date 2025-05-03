use crate::infrastructure::http::handlers::configuration::{
    handler_configuration_hovered, handler_configuration_set_active,
    handler_configuration_unhovered,
};
use axum::{
    Router,
    routing::{get, patch},
};

pub async fn configuration_router() -> Router {
    Router::new()
        .route("/unhovered", get(handler_configuration_unhovered))
        .route("/hovered", get(handler_configuration_hovered))
        .route("/active/{id}", patch(handler_configuration_set_active))
}
