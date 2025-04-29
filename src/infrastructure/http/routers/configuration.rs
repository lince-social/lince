use axum::{
    Router,
    routing::{get, patch},
};

use crate::infrastructure::http::handlers::configuration::{
    get_active_configuration_handler, get_inactive_configurations_handler,
    handler_configuration_set_active,
};

pub async fn configuration_router() -> Router {
    Router::new()
        .route("/unhovered", get(get_active_configuration_handler))
        .route("/hovered", get(get_inactive_configurations_handler))
        .route("/active/{id}", patch(handler_configuration_set_active))
}
