#[cfg(target_os = "espidf")]
mod device {
    use anyhow::{Context, ensure};
    use esp_idf_svc::sys;
    use lince_ailuros_esp_iroh::device::{resources, start};
    use std::time::{Duration, Instant};

    fn qualify_psram_atomics() -> anyhow::Result<()> {
        use std::sync::{
            Arc,
            atomic::{AtomicU32, Ordering},
        };
        let counter = Arc::new(AtomicU32::new(0));
        ensure!(
            unsafe { sys::esp_ptr_external_ram(Arc::as_ptr(&counter).cast()) },
            "Atomic qualification must exercise PSRAM"
        );
        let mut workers = Vec::new();
        for _ in 0..2 {
            let counter = counter.clone();
            workers.push(
                std::thread::Builder::new()
                    .stack_size(8192)
                    .spawn(move || {
                        for iteration in 0..10000 {
                            counter.fetch_add(1, Ordering::AcqRel);
                            let mut previous = counter.load(Ordering::Acquire);
                            loop {
                                match counter.compare_exchange(
                                    previous,
                                    previous + 1,
                                    Ordering::AcqRel,
                                    Ordering::Acquire,
                                ) {
                                    Ok(_) => break,
                                    Err(current) => previous = current,
                                }
                            }
                            if iteration % 128 == 0 {
                                std::thread::sleep(Duration::from_millis(1));
                            }
                        }
                    })?,
            );
        }
        for worker in workers {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("PSRAM atomic worker failed"))?;
        }
        ensure!(
            counter.load(Ordering::Acquire) == 40000,
            "PSRAM atomics produced an incorrect result"
        );
        println!(
            "{}",
            serde_json::json!({"type":"ailuros-psram-atomics","operations":40000,"ok":true})
        );
        Ok(())
    }

    pub fn run() -> anyhow::Result<()> {
        esp_idf_svc::sys::link_patches();
        esp_idf_svc::log::EspLogger::initialize_default();
        let device = start()?;
        device.wait_ready(Duration::from_secs(45))?;
        let secret = device.secret.clone();
        resources("network-ready");
        qualify_psram_atomics()?;
        let verification_started = Instant::now();
        lince_ailuros_esp_iroh::verification::qualify()?;
        println!(
            "{}",
            serde_json::json!({"type":"ailuros-signature-verification","ok":true,"elapsed_ms":verification_started.elapsed().as_millis()})
        );
        let events = esp_idf_svc::sys::esp_vfs_eventfd_config_t {
            max_fds: 5,
            ..Default::default()
        };
        esp_idf_svc::sys::esp!(unsafe { esp_idf_svc::sys::esp_vfs_eventfd_register(&events) })?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .thread_stack_size(8192)
            .build()?;
        runtime.block_on(async move {
            let endpoint = lince_ailuros_esp_iroh::endpoint(secret).await?;
            endpoint.set_alpns(vec![b"lince/ailuros-qualification/1".to_vec()]);
            resources("endpoint-bound");
            tokio::time::timeout(Duration::from_secs(45), endpoint.online()).await.context("Verified relay connection did not become ready")?;
            println!("{}", serde_json::json!({"type":"ailuros-iroh-qualification","endpoint":endpoint.id().to_string()}));
            let mut samples = tokio::time::interval(Duration::from_secs(5));
            loop {
                let incoming = tokio::select! {
                    incoming = endpoint.accept() => incoming,
                    _ = samples.tick() => { resources("online"); continue; }
                };
                let Some(incoming) = incoming else { break };
                let result = tokio::time::timeout(Duration::from_secs(10), async {
                    let connection = incoming.await?;
                    let (mut send, mut receive) = connection.accept_bi().await?;
                    let bytes = receive.read_to_end(256).await?;
                    send.write_all(&bytes).await?;
                    send.finish()?;
                    connection.closed().await;
                    Ok::<_, anyhow::Error>(())
                }).await;
                println!("{}", serde_json::json!({"type":"ailuros-iroh-qualification-result","ok":matches!(result,Ok(Ok(())))}));
                resources("exchange");
            }
            Ok(())
        })
    }
}

#[cfg(target_os = "espidf")]
fn main() {
    if let Err(error) = device::run() {
        eprintln!("Ailuros transport qualification failed: {error}");
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    eprintln!("Use the pinned ESP32-S3 toolchain for device transport qualification");
}
