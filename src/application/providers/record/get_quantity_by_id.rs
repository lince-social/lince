use crate::infrastructure::database::repositories::record::repository_record_get_quantity_by_id;

// pub async fn provider_record_get_quantity_by_id(id: u32) -> f64 {
//     repository_record_get_quantity_by_id(id).await
// }

pub fn provider_record_get_quantity_by_id_sync(id: u32) -> f64 {
    futures::executor::block_on(async {
        println!("asoiunaso");
        repository_record_get_quantity_by_id(id).await
    })
}
