use axum::extract::Query;

use crate::presentation::web::operation::operation::{get_operation, post_operation};

pub async fn get_operation_handler() -> String {
    get_operation().0
}

pub async fn post_operation_handler(operation: Query<String>) -> String {
    println!("operation in handler: {operation:?}");
    post_operation(operation.0).await.0
}
