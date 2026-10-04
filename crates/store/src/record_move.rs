pub mod offers;

use crate::StoreError;
use sqlx::SqlitePool;
use std::collections::HashSet;

pub async fn to_contact(pool: &SqlitePool, contact: &str) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query_scalar::<_,String>("SELECT m.record_uid FROM record_move_member m JOIN record_move_offer o ON o.uid=m.offer_uid WHERE o.peer=? AND o.direction='outgoing' AND o.state IN ('transferring','complete')")
        .bind(contact).fetch_all(pool).await?.into_iter().collect())
}

pub async fn waiting_for_contact(
    pool: &SqlitePool,
    contact: &str,
) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query_scalar::<_,String>("SELECT m.record_uid FROM record_move_member m JOIN record_move_offer o ON o.uid=m.offer_uid WHERE o.peer=? AND o.direction='outgoing' AND o.state IN ('offered','transferring','changed')")
        .bind(contact).fetch_all(pool).await?.into_iter().collect())
}
