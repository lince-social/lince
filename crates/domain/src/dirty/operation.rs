use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Deserialize)]
pub struct Operation {
    pub operation: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationActions {
    Create = 0,
    SQLQuery = 1,
    Karma = 2,
    Command = 3,
    ActivateConfiguration = 4,
}

impl OperationTables {
    pub fn from_id(id: u32) -> Option<Self> {
        match id {
            0 => Some(Self::Configuration),
            1 => Some(Self::Collection),
            2 => Some(Self::View),
            3 => Some(Self::CollectionView),
            4 => Some(Self::Record),
            5 => Some(Self::KarmaCondition),
            6 => Some(Self::KarmaConsequence),
            7 => Some(Self::Karma),
            8 => Some(Self::Command),
            9 => Some(Self::Frequency),
            10 => Some(Self::Sum),
            11 => Some(Self::History),
            12 => Some(Self::DNA),
            13 => Some(Self::Transfer),
            _ => None,
        }
    }

    pub fn as_table_name(&self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Collection => "collection",
            Self::View => "view",
            Self::CollectionView => "collection_view",
            Self::Record => "record",
            Self::KarmaCondition => "karma_condition",
            Self::KarmaConsequence => "karma_consequence",
            Self::Karma => "karma",
            Self::Command => "command",
            Self::Frequency => "frequency",
            Self::Sum => "sum",
            Self::History => "history",
            Self::DNA => "dna",
            Self::Transfer => "transfer",
        }
    }
}

impl FromStr for OperationTables {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "configuration" | "config" => Ok(Self::Configuration),
            "collection" | "collections" => Ok(Self::Collection),
            "view" | "views" => Ok(Self::View),
            "collection_view" | "collectionview" | "collection-view" | "cv" => {
                Ok(Self::CollectionView)
            }
            "record" | "records" => Ok(Self::Record),
            "karma_condition" | "karma-condition" | "kc" => Ok(Self::KarmaCondition),
            "karma_consequence" | "karma-consequence" | "ks" => Ok(Self::KarmaConsequence),
            "karma" => Ok(Self::Karma),
            "command" | "commands" => Ok(Self::Command),
            "frequency" | "frequencies" => Ok(Self::Frequency),
            "sum" | "sums" => Ok(Self::Sum),
            "history" => Ok(Self::History),
            "dna" => Ok(Self::DNA),
            "transfer" | "transfers" => Ok(Self::Transfer),
            _ => Err(format!("Unknown operation table: {s}")),
        }
    }
}

impl FromStr for OperationActions {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "c" | "create" => Ok(OperationActions::Create),
            "q" | "query" | "sql" => Ok(OperationActions::SQLQuery),
            "k" | "karma" => Ok(OperationActions::Karma),
            "s" | "command" | "shell" => Ok(OperationActions::Command),
            "a" | "configuration" => Ok(OperationActions::ActivateConfiguration),
            _ => Err(format!("Unknown operation action: {}", s)),
        }
    }
}

#[derive(Deserialize, Serialize, FromRow)]
pub struct Query {
    pub query: String,
}
