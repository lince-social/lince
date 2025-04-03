#[derive(sqlx::FromRow, Debug, PartialEq)]
pub struct Record {
    pub id: u32,
    pub quantity: f32,
    pub head: String,
    pub body: String,
}
