use crate::{
    application::schema::view::queried_view::QueriedView, domain::entities::collection::Collection,
};

pub type CollectionRow = (Collection, Vec<QueriedView>);
