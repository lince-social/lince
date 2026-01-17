use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Deserialize)]
pub struct Operation {
    pub operation: String,
}

#[derive(Deserialize, Serialize, FromRow)]
pub struct Query {
    pub query: String,
}
