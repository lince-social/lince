use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Deserialize)]
pub struct Operation {
    pub operation: String,
}

pub enum OperationTables {
    Configuration = 0,
    Collection = 1,
    View = 2,
    CollectionView = 3,
    Record = 4,
    KarmaCondition = 5,
    KarmaConsequence = 6,
    Karma = 7,
    Command = 8,
    Frequency = 9,
    Sum = 10,
    History = 11,
    DNA = 12,
    Transfer = 13,
}
pub enum OperationActions {
    Create = 0,
    SQLQuery = 1,
    Karma = 2,
    Command = 3,
    ActivateConfiguration = 4,
}

impl FromStr for OperationActions {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "q" | "query" | "sql" => Ok(OperationActions::SQLQuery),
            "k" | "karma" => Ok(OperationActions::Karma),
            "c" | "command" => Ok(OperationActions::Command),
            "a" | "configuration" => Ok(OperationActions::ActivateConfiguration),
            _ => Err(format!("Unknown operation action: {}", s)),
        }
    }
}

#[derive(Deserialize, Serialize, FromRow)]
pub struct Query {
    pub query: String,
}
