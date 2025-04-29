use crate::{
    application::providers::karma::get_deliver::provider_karma_get_deliver,
    domain::entities::karma::Karma,
};
use std::io::Error;

pub fn use_case_karma_get() -> Result<Vec<Karma>, Error> {
    futures::executor::block_on(async { provider_karma_get_deliver().await })
}
