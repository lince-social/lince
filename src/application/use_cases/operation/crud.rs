use std::collections::HashMap;

use crate::{
    application::providers::operation::crud::{
        provider_operation_create, provider_operation_get_column_names,
    },
    presentation::web::operation::create::presentation_web_create,
};

pub async fn use_case_operation_create_component(table: String) -> String {
    let column_names = provider_operation_get_column_names(table.clone()).await;
    presentation_web_create(table, column_names).await.0
}

pub async fn use_case_operation_create_persist(table: String, data: HashMap<String, String>) {
    provider_operation_create(table, data).await
}
