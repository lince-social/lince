use {crate::domain::widget_bridge::WidgetBridgeSnapshot, std::sync::Arc, tokio::sync::RwLock};

#[derive(Clone, Default)]
pub struct WidgetBridgeStore {
    snapshot: Arc<RwLock<WidgetBridgeSnapshot>>,
}

impl WidgetBridgeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn snapshot(&self) -> WidgetBridgeSnapshot {
        self.snapshot.read().await.clone()
    }

    pub async fn record_print(
        &self,
        instance_id: impl Into<String>,
        label: impl Into<String>,
    ) -> WidgetBridgeSnapshot {
        let instance_id = instance_id.into();
        let label = label.into();
        let mut snapshot = self.snapshot.write().await;
        snapshot.print_count += 1;
        snapshot.last_source = instance_id.clone();
        snapshot.last_message = format!("{instance_id} pediu print: {label}");
        tracing::info!("widget bridge print from {instance_id}: {label}");
        snapshot.clone()
    }
}
