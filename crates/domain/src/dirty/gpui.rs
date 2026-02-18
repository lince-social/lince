use crate::{
    clean::{pinned_view::PinnedView, table::Table},
    dirty::{collection::CollectionRow, view::ViewWithPinInfo},
};

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
    pub tables: Vec<(String, Table)>,
    pub special_views: Vec<String>,
    pub pinned_views: Vec<PinnedView>,
    pub pinned_tables: Vec<(String, Table)>,
    pub views_with_pin_info: Vec<ViewWithPinInfo>,
}
