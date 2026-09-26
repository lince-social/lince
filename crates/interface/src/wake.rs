use bevy::{
    prelude::*,
    winit::{EventLoopProxyWrapper, WinitUserEvent},
};
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
    pub(crate) fn after(&self, delay: std::time::Duration) {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let wake = self.clone();
            runtime.spawn(async move {
                tokio::time::sleep(delay).await;
                wake.ring();
            });
        }
    }
    pub fn from_proxy(proxy: &EventLoopProxyWrapper) -> Self {
        let proxy = (**proxy).clone();
        Self::new(move || {
            let _ = proxy.send_event(WinitUserEvent::WakeUp);
        })
    }
}
