use {
    crate::domain::lince_package::LincePackage,
    std::{
        collections::HashMap,
        sync::Arc,
        time::{Duration, Instant},
    },
    tokio::sync::RwLock,
    uuid::Uuid,
};

const PREVIEW_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone)]
pub struct PackagePreviewStore {
    previews: Arc<RwLock<HashMap<String, TimedPreview>>>,
}

#[derive(Debug, Clone)]
struct TimedPreview {
    package: LincePackage,
    created_at: Instant,
}

impl PackagePreviewStore {
    pub fn new() -> Self {
        Self {
            previews: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn store(&self, package: LincePackage) -> String {
        let preview_id = Uuid::new_v4().to_string();
        let mut previews = self.previews.write().await;
        retain_fresh(&mut previews);
        previews.insert(
            preview_id.clone(),
            TimedPreview {
                package,
                created_at: Instant::now(),
            },
        );
        preview_id
    }

    pub async fn get(&self, preview_id: &str) -> Option<LincePackage> {
        let mut previews = self.previews.write().await;
        retain_fresh(&mut previews);
        previews
            .get(preview_id)
            .map(|preview| preview.package.clone())
    }
}

fn retain_fresh(previews: &mut HashMap<String, TimedPreview>) {
    previews.retain(|_, preview| preview.created_at.elapsed() < PREVIEW_TTL);
}
