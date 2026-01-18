use domain::clean::table::Table;
use persistence::repositories::collection::CollectionRow;

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
    pub tables: Vec<(String, Table)>,
}
