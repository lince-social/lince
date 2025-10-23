use std::collections::HashMap;

pub type Row = HashMap<String, String>;
pub type Table = Vec<Row>;
pub type SortedTables = Vec<(String, Table, Vec<String>)>;
