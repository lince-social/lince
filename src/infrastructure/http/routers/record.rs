use axum::{
    Router,
    routing::{delete, post},
};

use crate::infrastructure::http::handlers::record::{create_record_handler, delete_record_handler};

pub async fn record_router() -> Router {
    Router::new()
        .route("/{id}", delete(delete_record_handler))
        .route("/", post(create_record_handler))
}
