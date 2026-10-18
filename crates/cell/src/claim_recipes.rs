use engine::Engine;
use std::sync::Arc;
use std::time::Duration;

pub fn spawn(engine: Arc<Engine>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut bus = engine.subscribe();
        let mut changes = engine.watch_query_changes();
        let mut dirty = true;
        loop {
            if dirty {
                if engine.schedule_claim_recipes().await.is_err() {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                dirty = false;
            }
            match engine.refresh_claim_recipes_once().await {
                Ok(count) if count > 0 => continue,
                Ok(_) => {}
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
            let delay = engine
                .next_claim_recipe_refresh()
                .await
                .ok()
                .flatten()
                .map(|time| {
                    (time - nucleus::execution::now())
                        .to_std()
                        .unwrap_or(Duration::from_millis(1))
                })
                .unwrap_or(Duration::from_secs(3600));
            tokio::select! {
                changed=changes.changed()=>{
                    if changed.is_err(){break;}
                    dirty=true;
                }
                received=bus.recv()=>{
                    if matches!(received,Err(tokio::sync::broadcast::error::RecvError::Closed)){break;}
                    dirty=match received {Ok(fact)=>!engine.is_generated_claim_record(&fact.record_uid).await.unwrap_or(false),Err(_)=>true};
                    while let Ok(fact)=bus.try_recv(){dirty|=!engine.is_generated_claim_record(&fact.record_uid).await.unwrap_or(false);}
                }
                _=tokio::time::sleep(delay)=>{}
            }
            if dirty {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    })
}

#[cfg(test)]
mod tests;
