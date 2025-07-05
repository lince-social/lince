use crate::{
    application::schemas::karma_filters::KarmaFilters,
    domain::entities::{
        karma::Karma, karma_condition::KarmaCondition, karma_consequence::KarmaConsequence,
    },
};
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait KarmaRepository: Send + Sync {
    async fn get_condition(&self) -> Result<Vec<KarmaCondition>, Error>;
    async fn get_consequence(&self) -> Result<Vec<KarmaConsequence>, Error>;
    async fn get(&self, filters: KarmaFilters) -> Result<Vec<Karma>, Error>;
}
