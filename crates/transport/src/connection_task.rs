//! Tasks whose lifetime is tied to an open transport connection.

use tokio::{spawn, task::JoinHandle};

#[must_use = "dropping the task handle aborts its work"]
pub(crate) struct ConnectionTask(JoinHandle<()>);

impl ConnectionTask {
    pub(crate) fn spawn(work: impl Future<Output = ()> + Send + 'static) -> Self {
        Self(spawn(work))
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.0.is_finished()
    }

    pub(crate) async fn cancel(mut self) {
        self.0.abort();
        let _ = (&mut self.0).await;
    }
}

impl Drop for ConnectionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
