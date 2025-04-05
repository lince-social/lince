use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::{
    infrastructure::http::handlers::record::{create_record_handler, delete_record_handler},
    presentation::web::record::record::get_records_component,
};

pub async fn record_router() -> Router {
    println!("Router");
    Router::new()
        .route("/", get(get_records_component().await.0))
        .route("/{id}", delete(delete_record_handler))
        .route("/rada", post(create_record_handler))
}
