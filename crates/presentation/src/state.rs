use crate::{
    domain::clean::table::Table, infrastructure::database::repositories::collection::CollectionRow,
};

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
    pub tables: Vec<(String, Table)>,
}
