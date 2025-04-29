use axum::{
    Router,
    routing::{delete, get, patch},
};

use crate::infrastructure::http::handlers::table::{
    handler_table_delete_by_id, handler_table_editable_row, handler_table_patch_row,
};

pub async fn table_router() -> Router {
    // Router::new().route("/{query}", get(table))
    Router::new()
        .route("/{table}/{id}", delete(handler_table_delete_by_id))
        .route(
            "/{table}/{id}/{column}/{value}",
            get(handler_table_editable_row),
        )
        .route("/{table}/{id}/{column}", patch(handler_table_patch_row))
}
