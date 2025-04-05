use axum::{Form, extract::Path, response::Html};
use maud::Markup;

use crate::{
    application::providers::record::{
        create::provider_record_create, delete_by_id::provider_record_delete_by_id,
    },
    domain::entities::record::RecordSchemaCreate,
    presentation::web::{
        record::record::get_records_component,
        section::{body::body_component, main::main_component},
    },
};

pub async fn get_records_handler() -> String {
    get_records_component().await.0
}

pub async fn delete_record_handler(Path(id): Path<String>) -> axum::response::Html<String> {
    println!("delete record id: {}", id);
    provider_record_delete_by_id(id).await;
    Html(main_component().await.0)
}

pub async fn create_record_handler(Form(record): Form<RecordSchemaCreate>) -> Html<&'static str> {
    println!("{:?}", record);
    provider_record_create(record).await;
    body_component().await
}
