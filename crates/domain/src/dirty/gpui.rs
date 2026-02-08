use crate::{clean::table::Table, dirty::{collection::CollectionRow, view::QueriedView}};

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
    pub tables: Vec<(String, Table)>,
    pub pinned_views: Vec<QueriedView>,
    pub pinned_tables: Vec<(String, Table)>,
}
