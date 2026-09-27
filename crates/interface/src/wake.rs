use bevy::prelude::*;
use std::sync::Arc;

#[derive(Resource, Clone)]
pub struct WakeSignal(Arc<dyn Fn() + Send + Sync>);

impl WakeSignal {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }
    pub fn ring(&self) {
        (self.0)();
    }
    pub fn after(&self, delay: std::time::Duration) {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let wake = self.clone();
            runtime.spawn(async move {
                tokio::time::sleep(delay).await;
                wake.ring();
            });
        }
    }
}
