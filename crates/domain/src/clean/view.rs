use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct View {
    pub id: u32,
    pub name: String,
    pub query: String,
    pub pinned: i32,
    pub position_x: Option<f64>,
    pub position_y: Option<f64>,
    pub z_index: i32,
}
