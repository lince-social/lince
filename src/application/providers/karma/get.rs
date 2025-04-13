use crate::{
    domain::entities::karma::Karma,
    infrastructure::database::repositories::karma::repository_karma_get,
};

pub fn provider_karma_get() -> Vec<Karma> {
    futures::executor::block_on(async { repository_karma_get().await })
}
