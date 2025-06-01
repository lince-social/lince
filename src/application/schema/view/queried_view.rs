use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct QueriedView {
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub query: String,
}
#[derive(sqlx::FromRow)]
pub struct QueriedViewWithConfigId {
    pub collection_id: u32,
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub query: String,
}
