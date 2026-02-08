use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct PinnedView {
    pub id: u32,
    pub view_id: u32,
    pub position_x: f64,
    pub position_y: f64,
    pub z_index: i32,
}
