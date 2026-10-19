#[cfg(target_os = "espidf")]
fn run() -> anyhow::Result<()> {
    use lince_ailuros_esp_iroh::{device, endpoint, network::Network};
    use std::time::Duration;
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    unsafe {
        esp_idf_svc::sys::esp_log_level_set(
            c"*".as_ptr(),
            esp_idf_svc::sys::esp_log_level_t_ESP_LOG_WARN,
        )
    };
    let device = device::start()?;
    let setup = device.setup()?;
    let destination = device.destination()?;
    let (mut camera, fps) = device.camera()?;
    let events = esp_idf_svc::sys::esp_vfs_eventfd_config_t {
        max_fds: 5,
        ..Default::default()
    };
    esp_idf_svc::sys::esp!(unsafe { esp_idf_svc::sys::esp_vfs_eventfd_register(&events) })?;
    tokio::runtime::Builder::new_current_thread().enable_all().max_blocking_threads(1)
        .event_interval(1).global_queue_interval(1)
        .thread_stack_size(8192).build()?.block_on(async {
            let mut network = Network::default();
            let mut backoff = 1u64;
            loop {
                let station = device.network().filter(|_| device.ready());
                let Some(station) = station else {
                    network.update(None, &device.secret, endpoint).await?;
                    device::status(c"waiting-for-wifi-and-clock");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                };
                let started = std::time::Instant::now();
                let result = async {
                    if network.update(Some(station), &device.secret, endpoint).await? {
                        let bound = network.endpoint().ok_or_else(|| anyhow::anyhow!("Device network endpoint unavailable"))?;
                        device::report(serde_json::json!({"type":"ailuros-camera-endpoint",
                            "endpoint":bound.id().to_string(),"addresses":bound.addr().ip_addrs().cloned().collect::<Vec<_>>(),
                            "generation":station.generation}));
                        backoff = 1;
                    }
                    let bound = network.endpoint().ok_or_else(|| anyhow::anyhow!("Device network endpoint unavailable"))?;
                    device::publish(&device, station, bound, destination, &setup, &mut camera, fps).await
                }.await;
                device::status(c"cell-connection-failed");
                device::report(serde_json::json!({"type":"ailuros-camera-disconnected","error":result.err().map(|error|error.to_string())}));
                device::resources("reconnect");
                if started.elapsed() >= Duration::from_secs(30) { backoff = 1; }
                let jitter = u64::from(unsafe { esp_idf_svc::sys::esp_random() }) % 1000;
                tokio::time::sleep(Duration::from_millis(backoff * 1000 + jitter)).await;
                backoff = (backoff * 2).min(30);
            }
        })
}

#[cfg(target_os = "espidf")]
fn main() {
    if let Err(error) = run() {
        eprintln!("Ailuros camera startup failed: {error}");
    }
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    eprintln!("Use the pinned ESP32-S3 toolchain for camera firmware");
}
