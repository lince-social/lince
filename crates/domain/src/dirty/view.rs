use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct QueriedView {
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub query: String,
}
impl Default for QueriedView {
    fn default() -> Self {
        Self {
            id: 0,
            quantity: 1,
            name: "Default View".to_string(),
            query: "SELECT * FROM record".to_string(),
        }
    }
}
