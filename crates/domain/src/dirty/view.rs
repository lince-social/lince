use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct QueriedView {
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub query: String,
    pub pinned: i32,
    pub position_x: Option<f64>,
    pub position_y: Option<f64>,
    pub z_index: i32,
}
impl Default for QueriedView {
    fn default() -> Self {
        Self {
            id: 0,
            quantity: 1,
            name: "Default View".to_string(),
            query: "SELECT * FROM record".to_string(),
            pinned: 0,
            position_x: None,
            position_y: None,
            z_index: 0,
        }
    }
}
