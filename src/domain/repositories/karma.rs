use crate::domain::entities::{
    karma::Karma, karma_condition::KarmaCondition, karma_consequence::KarmaConsequence,
};
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait KarmaRepository: Send + Sync {
    async fn get_condition(&self) -> Result<Vec<KarmaCondition>, Error>;
    async fn get_consequence(&self) -> Result<Vec<KarmaConsequence>, Error>;
    async fn get(&self, condition_record_id: Option<u32>) -> Result<Vec<Karma>, Error>;
}
