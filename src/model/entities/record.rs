#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct Record {
    id: u32,
    quantity: i32,
    head: String,
    body: String,
}
