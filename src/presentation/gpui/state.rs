use crate::infrastructure::database::repositories::collection::CollectionRow;

#[derive(Clone, Debug)]
pub struct State {
    pub collections: Vec<CollectionRow>,
}
