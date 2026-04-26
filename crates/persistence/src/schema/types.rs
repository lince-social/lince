#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub name: &'static str,
    pub sql_type: &'static str,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub default_sql: Option<&'static str>,
    pub references_sql: Option<&'static str>,
    pub check_sql: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    pub name: &'static str,
    pub columns: Vec<&'static str>,
    pub unique: bool,
    pub where_sql: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    pub name: &'static str,
    pub strict: bool,
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
    pub checks: Vec<&'static str>,
    pub composite_primary_key: Option<Vec<&'static str>>,
}

pub trait Table {
    fn schema() -> TableSchema;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveColumn {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub primary_key_position: i64,
    pub default_sql: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveIndex {
    pub name: String,
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveTable {
    pub name: String,
    pub strict: bool,
    pub columns: Vec<LiveColumn>,
    pub indexes: Vec<LiveIndex>,
}
