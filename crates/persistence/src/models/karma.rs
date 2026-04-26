use lince_persistence_table_derive::Table;

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "karma_condition")]
pub struct KarmaConditionRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: i64,
    #[table(default = "'Condition'")]
    pub name: String,
    pub condition: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "karma_consequence")]
pub struct KarmaConsequenceRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: i64,
    #[table(default = "'Consequence'")]
    pub name: String,
    pub consequence: String,
}

#[derive(Table, sqlx::FromRow, Debug, Clone, PartialEq)]
#[table(name = "karma")]
pub struct KarmaRow {
    #[table(primary_key)]
    pub id: i64,
    #[table(default = "1")]
    pub quantity: i64,
    #[table(default = "'Karma'")]
    pub name: String,
    pub condition_id: i64,
    pub operator: String,
    pub consequence_id: i64,
}
