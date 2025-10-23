use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Deserialize, Serialize, Debug, Clone, FromRow)]
pub struct KarmaView {
    pub karma_id: u32,
    pub karma_name: String,
    pub karma_quantity: i32,

    pub karma_condition_value: Option<String>,
    pub karma_condition_explanation: String,
    pub karma_condition_condition: String,
    pub karma_condition_name: String,
    pub karma_condition_quantity: i32,
    pub karma_condition_id: u32,

    pub karma_operator: String,

    pub karma_consequence_id: u32,
    pub karma_consequence_quantity: i32,
    pub karma_consequence_name: String,
    pub karma_consequence_consequence: String,
    pub karma_consequence_explanation: String,
    pub karma_consequence_value: Option<String>,
}
#[derive(Deserialize, Serialize, Debug, Clone, FromRow)]
pub struct KarmaConditionView {
    pub value: Option<String>,
    pub explanation: String,
    pub condition: String,
    pub name: String,
    pub quantity: i32,
    pub id: u32,
}

#[derive(Deserialize, Serialize, Debug, Clone, FromRow)]
pub struct KarmaConsequenceView {
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub consequence: String,
    pub explanation: String,
    pub value: Option<String>,
}
