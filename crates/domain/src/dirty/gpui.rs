use crate::{clean::{table::Table, pinned_view::PinnedView}, dirty::{collection::CollectionRow, view::ViewWithPinInfo}};

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
    pub tables: Vec<(String, Table)>,
    pub pinned_views: Vec<PinnedView>,
    pub pinned_tables: Vec<(String, Table)>,
    pub views_with_pin_info: Vec<ViewWithPinInfo>,
}
