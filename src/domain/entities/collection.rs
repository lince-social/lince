#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct Collection {
    pub id: u32,
    pub name: String,
    pub quantity: i32,
}
