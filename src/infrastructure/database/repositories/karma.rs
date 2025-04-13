use crate::{
    domain::entities::karma::Karma, infrastructure::database::management::lib::connection,
};

pub async fn repository_karma_get() -> Vec<Karma> {
    let pool = connection().await.unwrap();

    let query = "
            SELECT
                k.id,
                k.quantity,
                kcd.condition,
                k.operator,
                kcs.consequence
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            WHERE k.quantity > 0 AND kcd.quantity > 0 AND kcs.quantity > 0;
            ";

    let data: Vec<Karma> = sqlx::query_as(query).fetch_all(&pool).await.unwrap();
    data
}
