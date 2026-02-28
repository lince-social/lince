use crate::{
    clean::{pinned_view::PinnedView, table::Table},
    dirty::{collection::CollectionRow, view::ViewWithPinInfo},
};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
    pub tables: Vec<(u32, String, Table)>,
    pub special_views: Vec<String>,
    pub pinned_views: Vec<PinnedView>,
    pub pinned_tables: Vec<(u32, String, Table)>,
    pub views_with_pin_info: Vec<ViewWithPinInfo>,
    pub collection_view_column_widths: HashMap<u32, HashMap<String, f32>>,
}
