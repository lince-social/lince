use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetBridgeSnapshot {
    pub print_count: u64,
    pub last_source: String,
    pub last_message: String,
}

impl Default for WidgetBridgeSnapshot {
    fn default() -> Self {
        Self {
            print_count: 0,
            last_source: "nenhum".into(),
            last_message: "Aguardando interacao entre widgets.".into(),
        }
    }
}
