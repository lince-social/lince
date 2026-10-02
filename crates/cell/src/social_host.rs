use std::{io, path::Path, sync::Arc};

use engine::{Engine, social::Worker};
use nucleus::social::ServiceSettings;
use tokio::io::AsyncReadExt;

const MAX_CONFIG_BYTES: u64 = 8192;

pub async fn load(path: &Path) -> io::Result<ServiceSettings> {
    let file = tokio::fs::File::open(path).await?;
    if !file.metadata().await?.is_file() {
        return Err(io::Error::other(
            "Social hosting configuration must be a regular JSON file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(io::Error::other(
            "Social hosting configuration exceeds 8 KiB",
        ));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

pub async fn configure(engine: &Engine) -> io::Result<()> {
    if let Some(path) = std::env::var_os("LINCE_SOCIAL_CONFIG") {
        engine
            .social_set_deployment_settings(load(Path::new(&path)).await?)
            .map_err(io::Error::other)?;
    }
    Ok(())
}

struct Child(tokio::task::JoinHandle<()>);

impl Drop for Child {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub fn supervise<F>(
    engine: Arc<Engine>,
    worker: Worker,
    mut spawn: F,
) -> tokio::task::JoinHandle<()>
where
    F: FnMut() -> tokio::task::JoinHandle<()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut backoff = 3;
        loop {
            engine.social_worker_started(worker);
            let started = tokio::time::Instant::now();
            let mut child = Child(spawn());
            let _ = (&mut child.0).await;
            engine.social_worker_stopped(worker);
            tracing::warn!(
                worker = worker.name(),
                "Social worker stopped; durable work will resume after restart"
            );
            if started.elapsed().as_secs() >= 60 {
                backoff = 3;
            }
            tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
            backoff = (backoff * 2).min(60);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn config_is_bounded_strict_and_does_not_apply_invalid_input() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("social.json");
        let engine = Engine::open_memory().await.unwrap();
        assert!(!engine.social_services_managed());
        let mut settings = ServiceSettings {
            directory: true,
            ..Default::default()
        };
        std::fs::write(&path, serde_json::to_vec(&settings).unwrap()).unwrap();
        engine
            .social_set_deployment_settings(load(&path).await.unwrap())
            .unwrap();
        assert!(engine.social_settings().await.unwrap().directory);
        assert!(engine.social_services_managed());
        settings.mailbox = true;
        settings.storage_bytes = 1;
        assert!(engine.social_set_deployment_settings(settings).is_err());
        assert!(!engine.social_settings().await.unwrap().mailbox);
        std::fs::write(&path, b"{\"surprise\":true}").unwrap();
        assert!(load(&path).await.is_err());
        std::fs::write(&path, vec![b' '; MAX_CONFIG_BYTES as usize + 1]).unwrap();
        assert!(load(&path).await.is_err());
        assert!(load(directory.path()).await.is_err());
    }

    #[tokio::test]
    async fn supervision_restarts_failed_work_and_cancels_the_current_child() {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        struct Active(Arc<AtomicUsize>);
        impl Drop for Active {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let supervisor = supervise(engine.clone(), Worker::Gossip, {
            let attempts = attempts.clone();
            let active = active.clone();
            move || {
                let number = attempts.fetch_add(1, Ordering::SeqCst);
                let active = active.clone();
                tokio::spawn(async move {
                    if number == 0 {
                        panic!("simulated worker failure");
                    }
                    active.fetch_add(1, Ordering::SeqCst);
                    let _guard = Active(active);
                    std::future::pending::<()>().await;
                })
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(6), async {
            while active.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        let health = engine.social_service_health().await.unwrap();
        assert_eq!(
            health["service_health"]["workers"][0]["state"]["restarts"],
            1
        );
        supervisor.abort();
        let _ = supervisor.await;
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while active.load(Ordering::SeqCst) != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
