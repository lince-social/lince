use crate::infrastructure::database::repositories::collection::CollectionRow;
use gpui::{Entity, Global};

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
}

pub struct StateModel {
    pub inner: Entity<State>,
}

impl Global for StateModel {}
