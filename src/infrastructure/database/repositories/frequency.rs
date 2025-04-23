use crate::{
    domain::entities::frequency::Frequency, infrastructure::database::management::lib::connection,
};

pub async fn repository_frequency_get(id: u32) -> Option<Frequency> {
    let pool = connection().await.unwrap();

    let query = format!(
        "SELECT * FROM frequency WHERE id = {} and quantity <> 0",
        id
    );
    sqlx::query_as(&query).fetch_optional(&pool).await.unwrap()
}

pub async fn repository_frequency_update(frequency: Frequency) {
    let pool = connection().await.unwrap();

    let query = format!(
        "UPDATE frequency SET quantity = {}, next_date = '{}' WHERE id = {}",
        frequency.quantity, frequency.next_date, frequency.id
    );
    sqlx::query(&query).execute(&pool).await.unwrap();
}
