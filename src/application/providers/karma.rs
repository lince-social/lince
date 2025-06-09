use crate::domain::{
    entities::{karma::Karma, table::Table},
    repositories::karma::KarmaRepository,
};
use std::{io::Error, sync::Arc};

pub struct KarmaProvider {
    pub repository: Arc<dyn KarmaRepository>,
}

impl KarmaProvider {
    pub fn new(repository: Arc<dyn KarmaRepository>) -> Self {
        Self { repository }
    }

    pub async fn get(&self) -> Result<Vec<Karma>, Error> {
        self.repository.get().await
    }

    pub async fn get_condition(&self) -> Result<Vec<(String, Table)>, Error> {
        self.repository.get_condition().await
    }

    pub async fn get_consequence(&self) -> Result<Vec<(String, Table)>, Error> {
        self.repository.get_consequence().await
    }

    pub async fn get_joined(&self) -> Result<Vec<(String, Table)>, Error> {
        self.repository.get_joined().await
    }
}
