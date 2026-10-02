use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Subscription {
    pub id: String,
    pub label: String,
    pub query: Search,
    pub services: Vec<String>,
    pub interval_minutes: u32,
    pub enabled: bool,
    pub notifications: bool,
    pub quiet_start_hour: u32,
    pub quiet_end_hour: u32,
    #[serde(default)]
    pub actor: Option<String>,
}

impl Default for Subscription {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            query: Search::default(),
            services: Vec::new(),
            interval_minutes: 60,
            enabled: false,
            notifications: false,
            quiet_start_hour: 22,
            quiet_end_hour: 8,
            actor: None,
        }
    }
}
