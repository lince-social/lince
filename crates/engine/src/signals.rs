use chrono::{DateTime, Utc};
use nucleus::Fact;

use crate::{Engine, EngineError};

impl Engine {
    pub async fn sample_due_signals(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        self.advance_karma_time(now).await
    }
}
