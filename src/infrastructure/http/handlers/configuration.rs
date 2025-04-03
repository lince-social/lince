use crate::presentation::web::record::record::get_record;

pub async fn get_configuration_handler() -> String {
    get_record().await.0
}
