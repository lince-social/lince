use crate::{clean::collection::Collection, dirty::view::QueriedView};

pub type CollectionRow = (Collection, Vec<QueriedView>);
