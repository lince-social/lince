use crate::{
    application::schemas::karma_filters::KarmaFilters,
    domain::{
        entities::{
            karma::Karma, karma_condition::KarmaCondition, karma_consequence::KarmaConsequence,
        },
        repositories::karma::KarmaRepository,
    },
};
use std::{io::Error, sync::Arc};

pub struct KarmaProvider {
    pub repository: Arc<dyn KarmaRepository>,
}

impl KarmaProvider {
    pub async fn get(&self, filters: KarmaFilters) -> Result<Vec<Karma>, Error> {
        self.repository.get(filters).await
    }

    pub async fn get_condition(&self) -> Result<Vec<KarmaCondition>, Error> {
        self.repository.get_condition().await
    }

    pub async fn get_consequence(&self) -> Result<Vec<KarmaConsequence>, Error> {
        self.repository.get_consequence().await
    }
}
