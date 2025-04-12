use crate::{
    application::providers::record::{
        create::provider_record_create, delete_by_id::provider_record_delete_by_id,
    },
    domain::entities::record::RecordSchemaCreate,
    presentation::web::section::main::presentation_web_main,
};
use axum::{Form, extract::Path, response::Html};

pub async fn delete_record_handler(Path(id): Path<String>) -> axum::response::Html<String> {
    let _ = provider_record_delete_by_id(id).await;
    Html(presentation_web_main().await.0)
}

pub async fn create_record_handler(Form(record): Form<RecordSchemaCreate>) -> Html<String> {
    let _ = provider_record_create(record).await;
    Html(presentation_web_main().await.0)
}
