use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryChanges {
    pub general: u64,
    pub projection: u64,
}

impl QueryChanges {
    pub fn revision(&self) -> u64 {
        self.general.wrapping_add(self.projection)
    }
}

/// Query notifications for workers whose inputs exclude forecast results.
///
/// This uses the original watch channel without a forwarding task, secondary
/// channel or timer. A cancelled wait may consume forecast-only notifications;
/// a general change is recorded only when the wait returns it. Durable Facts
/// are still delivered separately through the Engine's broadcast channel.
pub struct GeneralQueryChanges {
    changes: watch::Receiver<QueryChanges>,
    observed: Option<u64>,
}

impl GeneralQueryChanges {
    pub(crate) fn new(changes: watch::Receiver<QueryChanges>) -> Self {
        let current = changes.borrow().general;
        // A notification may have arrived between subscription and this read.
        // Preserve an already-pending change conservatively on the first wait.
        let observed = match changes.has_changed() {
            Ok(false) => Some(current),
            _ => None,
        };
        Self { changes, observed }
    }

    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        loop {
            self.changes.changed().await?;
            let current = self.changes.borrow_and_update().general;
            let previous = self.observed.replace(current);
            if previous != Some(current) {
                crate::performance::add("general-query-changes-delivered", 1);
                return Ok(());
            }
            crate::performance::add("general-query-projection-only-skips", 1);
        }
    }
}
