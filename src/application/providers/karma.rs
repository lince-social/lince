use crate::domain::{entities::karma::Karma, repositories::karma::KarmaRepository};
use std::io::Error;

pub struct KarmaProvider {
    pub repository: std::sync::Arc<dyn KarmaRepository>,
}

impl KarmaProvider {
    pub fn new(repository: std::sync::Arc<dyn KarmaRepository>) -> Self {
        Self { repository }
    }

    pub async fn get_deliver(&self) -> Result<Vec<Karma>, Error> {
        self.repository.get_deliver().await
    }

    pub async fn get_condition(&self) -> Result<Option<String>, Error> {
        self.repository.get_condition().await
    }

    pub async fn get_consequence(&self) -> Result<Option<String>, Error> {
        self.repository.get_consequence().await
    }

    pub async fn get_joined(&self) -> Result<Option<String>, Error> {
        self.repository.get_joined().await
    }
}
