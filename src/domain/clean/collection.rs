#[derive(sqlx::FromRow, Clone, Debug, PartialEq, Eq)]
pub struct Collection {
    pub id: u32,
    pub name: String,
    pub quantity: i32,
}

impl Default for Collection {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Default Collection".to_string(),
            quantity: 1,
        }
    }
}

impl Collection {
    pub fn error() -> Self {
        Self {
            id: 0,
            name: "Error in Collection".to_string(),
            quantity: 1,
        }
    }
}
