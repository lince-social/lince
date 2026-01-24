use crate::{clean::table::Table, dirty::collection::CollectionRow};

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
    pub tables: Vec<(String, Table)>,
}
