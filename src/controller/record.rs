use axum::{Router, routing::get};

use crate::{
    model::database::repositories::record::create_record, view::web::record::record::get_record,
};

pub async fn configuration_router() -> Router {
    Router::new().route("/", get(get_record().await.0))
}
