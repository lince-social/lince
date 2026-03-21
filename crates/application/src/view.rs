use injection::cross_cutting::InjectedServices;
use persistence::repositories::view::ViewSnapshot;
use std::{collections::BTreeSet, io::Error};

#[derive(Debug, Clone)]
pub struct ViewSnapshotEnvelope {
    pub snapshot: ViewSnapshot,
    pub payload: String,
}

#[derive(Clone)]
pub struct ViewReadService {
    services: InjectedServices,
}

impl ViewReadService {
    pub fn new(services: InjectedServices) -> Self {
        Self { services }
    }

    pub async fn read_snapshot(&self, view_id: u32) -> Result<ViewSnapshotEnvelope, Error> {
        let snapshot = self.services.repository.view.read_snapshot(view_id).await?;
        let payload = serde_json::to_string(&snapshot).map_err(Error::other)?;
        Ok(ViewSnapshotEnvelope { snapshot, payload })
    }

    pub async fn dependencies(&self, view_id: u32) -> Result<BTreeSet<String>, Error> {
        self.services
            .repository
            .view
            .get_dependencies(view_id)
            .await
    }
}
