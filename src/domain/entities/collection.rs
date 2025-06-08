#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct Collection {
    pub id: u32,
    pub name: String,
    pub quantity: i32,
    pub language: Option<String>,
    pub timezone: Option<String>,
    pub style: Option<String>,
}
