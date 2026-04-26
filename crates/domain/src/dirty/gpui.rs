use crate::{clean::table::Table, dirty::collection::CollectionRow};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
    pub tables: Vec<(u32, String, Table)>,
    pub special_views: Vec<String>,
    pub collection_view_column_widths: HashMap<u32, HashMap<String, f32>>,
}
