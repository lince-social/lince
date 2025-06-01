use crate::infrastructure::http::handlers::selection::{
    handler_selection_hovered, handler_selection_set_active, handler_selection_unhovered,
};
use axum::{
    Router,
    routing::{get, patch},
};

pub async fn selection_router() -> Router {
    Router::new()
        .route("/unhovered", get(handler_selection_unhovered))
        .route("/hovered", get(handler_selection_hovered))
        .route("/active/{id}", patch(handler_selection_set_active))
}
