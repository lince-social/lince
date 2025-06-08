use std::collections::HashMap;

use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::{
        operation::create::presentation_web_create, section::main::presentation_web_section_main,
    },
};

pub async fn use_case_operation_create_component(services: InjectedServices, table: String) -> String {
    let column_names = services.providers.operation.get_column_names(table.clone()).await.unwrap();
    presentation_web_create(table, column_names).await.0
}

pub async fn use_case_operation_create_persist(
    services: InjectedServices,
    table: String,
    data: HashMap<String, String>,
) -> String {
    if let Err(e) = services.providers.operation.create(table, data).await {
        println!("Error creating operation: {}", e);
    }
    presentation_web_section_main(services).await
}
