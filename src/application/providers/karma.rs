use crate::domain::{
    entities::{
        karma::Karma, karma_condition::KarmaCondition, karma_consequence::KarmaConsequence,
    },
    repositories::karma::KarmaRepository,
};
use std::{io::Error, sync::Arc};

pub struct KarmaProvider {
    pub repository: Arc<dyn KarmaRepository>,
}

impl KarmaProvider {
    pub async fn get(&self) -> Result<Vec<Karma>, Error> {
        self.repository.get().await
    }

    pub async fn get_condition(&self) -> Result<Vec<KarmaCondition>, Error> {
        self.repository.get_condition().await
    }

    pub async fn get_consequence(&self) -> Result<Vec<KarmaConsequence>, Error> {
        self.repository.get_consequence().await
    }
}
