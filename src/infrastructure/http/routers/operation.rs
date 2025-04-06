use axum::{
    Router,
    routing::{get, post},
};

use crate::infrastructure::http::handlers::operation::{
    get_operation_handler, handler_operation_create, post_operation_handler,
};

pub async fn operation_router() -> Router {
    Router::new()
        .route("/", get(get_operation_handler))
        .route("/", post(post_operation_handler))
        .route("/create/{table}", post(handler_operation_create))
}
