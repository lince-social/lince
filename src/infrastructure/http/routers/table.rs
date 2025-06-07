use axum::{
    Router,
    routing::{delete, patch, post},
};

use crate::infrastructure::{
    cross_cutting::InjectedServices,
    http::handlers::table::{
        handler_table_delete_by_id, handler_table_editable_row, handler_table_patch_row,
    },
};

pub fn table_router(services: InjectedServices) -> Router {
    Router::new()
        .route("/{table}/{id}", delete(handler_table_delete_by_id))
        .route("/{table}/{id}/{column}", post(handler_table_editable_row))
        .route("/{table}/{id}/{column}", patch(handler_table_patch_row))
        .with_state(services)
}
