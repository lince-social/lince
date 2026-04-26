use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub default_sql: Option<String>,
    pub references_sql: Option<String>,
    pub check_sql: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDef {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub where_sql: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: String,
    pub strict: bool,
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
    pub checks: Vec<String>,
    pub composite_primary_key: Option<Vec<String>>,
}

pub trait Table {
    fn schema() -> TableSchema;
}
