use crate::infrastructure::http::handlers::operation::{
    get_operation_handler, handler_operation_create, handler_operation_execute_query,
    post_operation_handler,
};
use axum::{
    Router,
    routing::{get, post},
};

pub async fn operation_router() -> Router {
    Router::new()
        .route("/", get(get_operation_handler))
        .route("/", post(post_operation_handler))
        .route("/query", post(handler_operation_execute_query))
        .route("/create/{table}", post(handler_operation_create))
}
