use serde::Deserialize;

#[derive(Deserialize)]
pub struct Operation {
    pub operation: String,
}
