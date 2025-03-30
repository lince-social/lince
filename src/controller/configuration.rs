use axum::{Router, routing::get};

use crate::view::web::{configuration::configurations::unhovered, record::record::get_record};

pub async fn configuration_router() -> Router {
    Router::new().route("/unhovered", get(get_record().await.0))
}
