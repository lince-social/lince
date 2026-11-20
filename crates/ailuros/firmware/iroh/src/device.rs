use anyhow::{Context, ensure};
use esp_idf_svc::{
    nvs::{EspDefaultNvsPartition, EspNvs},
    sntp::SyncStatus,
    sys::{self, ailuros},
};
use std::time::{Duration, Instant};

mod camera;
pub use camera::Camera;

pub struct Runtime {
    configuration: *mut ailuros::cJSON,
    readiness: crate::readiness::Readiness,
    pub secret: iroh::SecretKey,
}

impl Runtime {
    fn field(&self, name: &std::ffi::CStr) -> anyhow::Result<serde_json::Value> {
        // SAFETY: Startup retains the validated configuration, and `name` is NUL-terminated.
        let bytes = unsafe {
            ailuros::cJSON_PrintUnformatted(ailuros::cJSON_GetObjectItemCaseSensitive(
                self.configuration,
                name.as_ptr(),
            ))
        };
        ensure!(!bytes.is_null(), "Missing provisioned device setting");
        // SAFETY: cJSON returned an owned NUL-terminated string that stays live until freed below.
        let value = serde_json::from_slice(unsafe { std::ffi::CStr::from_ptr(bytes).to_bytes() });
        // SAFETY: Deserialization returned an owned value and cJSON allocated this buffer.
        unsafe { ailuros::cJSON_free(bytes.cast()) };
        value.context("Invalid provisioned device setting")
    }

    pub fn setup(&self) -> anyhow::Result<crate::client::Setup> {
        let mut setup = self.field(c"setup")?;
        let epoch: i64 = setup["key_epoch"]
            .as_str()
            .context("Missing enrolled key epoch")?
            .parse()?;
        setup["key_epoch"] = epoch.into();
        let setup: crate::client::Setup = serde_json::from_value(setup)?;
        setup.validate()?;
        Ok(setup)
    }

    pub fn destination(&self) -> anyhow::Result<iroh::EndpointId> {
        let destination = self.field(c"destination")?;
        ensure!(
            destination["transport"] == "iroh",
            "Install the firmware for the selected transport"
        );
        destination["endpoint_id"]
            .as_str()
            .context("Missing destination Cell identity")?
            .parse()
            .context("Invalid destination Cell identity")
    }

    pub fn camera(&self) -> anyhow::Result<(Camera, u32)> {
        let workflow = self.field(c"workflow")?;
        ensure!(
            workflow["kind"] == "camera",
            "Choose a camera workflow for this firmware"
        );
        let fps = workflow["fps"]
            .as_u64()
            .context("Missing camera frame rate")?;
        Ok((
            Camera::new(serde_json::to_vec(&workflow)?)?,
            u32::try_from(fps)?,
        ))
    }

    pub fn ready(&self) -> bool {
        self.readiness.ready(
            // SAFETY: Runtime startup initialized the C network state before creating `self`.
            unsafe { ailuros::lince_network_connected() },
            || {
                // SAFETY: Readiness is checked after startup initialized the SNTP service.
                SyncStatus::from(unsafe { sys::sntp_get_sync_status() }) == SyncStatus::Completed
            },
            // SAFETY: This reads the C runtime's initialized clock state without borrowing it.
            unsafe { ailuros::lince_time_ready() },
        )
    }

    pub fn network(&self) -> Option<crate::network::Snapshot> {
        // SAFETY: The C runtime maintains the network generation as an atomic value.
        let generation = unsafe { ailuros::lince_network_generation() };
        // SAFETY: Runtime startup initialized the C network state before creating `self`.
        if !unsafe { ailuros::lince_network_connected() } {
            return None;
        }
        // SAFETY: The key is a NUL-terminated string and startup initialized ESP-NETIF.
        let interface = unsafe { sys::esp_netif_get_handle_from_ifkey(c"WIFI_STA_DEF".as_ptr()) };
        if interface.is_null() {
            return None;
        }
        // SAFETY: The C struct contains only IPv4 integers, for which all-zero is valid.
        let mut information = unsafe { std::mem::zeroed::<sys::esp_netif_ip_info_t>() };
        // SAFETY: Startup keeps the station interface live across reconnects. The output is writable.
        if unsafe { sys::esp_netif_get_ip_info(interface, &mut information) } != sys::ESP_OK {
            return None;
        }
        // SAFETY: Read the same initialized network state again to detect a change during the query.
        let unchanged = unsafe {
            generation == ailuros::lince_network_generation() && ailuros::lince_network_connected()
        };
        if !unchanged {
            return None;
        }
        Some(crate::network::Snapshot {
            generation,
            address: std::net::Ipv4Addr::from(information.ip.addr.to_ne_bytes()),
            gateway: std::net::Ipv4Addr::from(information.gw.addr.to_ne_bytes()),
            netmask: std::net::Ipv4Addr::from(information.netmask.addr.to_ne_bytes()),
        })
    }

    async fn network_changed(&self, expected: crate::network::Snapshot) {
        loop {
            if !self.ready() || self.network() != Some(expected) {
                return;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    pub fn wait_ready(&self, timeout: Duration) -> anyhow::Result<()> {
        let deadline = Instant::now() + timeout;
        while !self.ready() {
            ensure!(
                Instant::now() < deadline,
                "Wi-Fi and clock synchronization required for verified relay TLS"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

pub fn resources(stage: &str) {
    let internal = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
    let mut power_save = 0;
    // SAFETY: `power_save` is writable and the SDK reports failure if Wi-Fi is unavailable.
    let power_save =
        (unsafe { sys::esp_wifi_get_ps(&mut power_save) } == sys::ESP_OK).then_some(power_save);
    // SAFETY: These SDK counters take valid capability flags or no arguments. FreeRTOS
    // interprets a null task handle as the current task, and this runs in a task.
    let report = unsafe {
        serde_json::json!({
            "type": "ailuros-iroh-resources", "stage": stage,
            "monotonic_ms": sys::esp_timer_get_time() / 1000,
            "verification_cache": crate::verification::cache_statistics(),
            "wifi_power_save": power_save,
            "free_internal": sys::heap_caps_get_free_size(internal),
            "minimum_internal": sys::heap_caps_get_minimum_free_size(internal),
            "largest_internal": sys::heap_caps_get_largest_free_block(internal),
            "free_psram": sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM),
            "largest_dma": sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_DMA),
            "stack_free_bytes": sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()),
        })
    };
    self::report(report);
}

pub fn report(value: serde_json::Value) {
    if let Ok(mut bytes) = serde_json::to_vec(&value) {
        bytes.push(b'\n');
        // SAFETY: UART0 is the firmware console and the byte slice stays live through the write.
        unsafe { sys::uart_write_bytes(0, bytes.as_ptr().cast(), bytes.len()) };
    }
}

pub fn start() -> anyhow::Result<Runtime> {
    let mut configuration = std::ptr::null_mut();
    // SAFETY: The firmware calls startup once and provides a writable configuration slot.
    let started = unsafe { ailuros::lince_runtime_start(&mut configuration) };
    sys::esp!(started)
        .context("Shared device startup failed; inspect Ailuros USB status")?;
    let secret = identity(EspDefaultNvsPartition::take_with(false)?)?;
    Ok(Runtime {
        configuration,
        readiness: Default::default(),
        secret,
    })
}

struct Identity;

impl crate::client::Identity for Identity {
    fn confirm_time(&mut self, time: i64) -> anyhow::Result<()> {
        // SAFETY: Startup initialized the clock state and the C API checks the timestamp range.
        let result = unsafe { ailuros::lince_time_from_server(time) };
        sys::esp!(result)?;
        Ok(())
    }

    fn sign(&mut self, payload: &str) -> anyhow::Result<String> {
        let payload = std::ffi::CString::new(payload)?;
        let mut signature = [0 as std::ffi::c_char; 129];
        // SAFETY: Startup initialized the signing context. The input is NUL-terminated and
        // the output has the 129 writable bytes required by lince_sign.
        let result = unsafe { ailuros::lince_sign(payload.as_ptr(), signature.as_mut_ptr()) };
        sys::esp!(result)?;
        // SAFETY: Successful signing writes 128 hexadecimal characters and a NUL terminator.
        Ok(unsafe { std::ffi::CStr::from_ptr(signature.as_ptr()) }
            .to_str()?
            .into())
    }
}

pub fn status(value: &'static std::ffi::CStr) {
    // SAFETY: The C runtime retains this NUL-terminated pointer, which has static storage.
    unsafe { ailuros::lince_status_set(value.as_ptr()) };
}

struct Connection(iroh::endpoint::Connection);

impl Drop for Connection {
    fn drop(&mut self) {
        self.0.close(0u32.into(), b"camera session ended");
    }
}

pub async fn publish(
    runtime: &Runtime,
    network: crate::network::Snapshot,
    endpoint: &iroh::Endpoint,
    destination: iroh::EndpointId,
    setup: &crate::client::Setup,
    camera: &mut Camera,
    fps: u32,
) -> anyhow::Result<()> {
    let result = tokio::select! {
        result = publish_session(endpoint, destination, setup, camera, fps) => result,
        _ = runtime.network_changed(network) => Err(anyhow::anyhow!("Wi-Fi network or clock readiness changed")),
    };
    let cleanup = tokio::time::timeout(
        Duration::from_secs(2),
        crate::client::Camera::set_active(camera, false),
    )
    .await
    .context("Camera cleanup timed out")?;
    result.and(cleanup)
}

async fn publish_session(
    endpoint: &iroh::Endpoint,
    destination: iroh::EndpointId,
    setup: &crate::client::Setup,
    camera: &mut Camera,
    fps: u32,
) -> anyhow::Result<()> {
    status(c"connecting-cell");
    let connection = Connection(
        tokio::time::timeout(
            Duration::from_secs(60),
            endpoint.connect(destination, ailuros_protocol::ALPN),
        )
        .await
        .context("Cell address lookup or connection timed out")??,
    );
    ensure!(
        connection.0.remote_id() == destination,
        "Destination Cell identity changed"
    );
    let (send, receive) = tokio::time::timeout(Duration::from_secs(5), connection.0.accept_bi())
        .await
        .context("Cell did not open its hardware stream")??;
    let path = connection
        .0
        .paths()
        .iter()
        .find(|path| path.is_selected())
        .map(|path| if path.is_relay() { "relay" } else { "direct" });
    report(
        serde_json::json!({"type":"ailuros-cell-connection","endpoint":destination.to_string(),"path":path}),
    );
    let result = {
        let mut identity = Identity;
        let stream = crate::client::publish(receive, send, setup, fps, &mut identity, camera);
        tokio::pin!(stream);
        let mut samples = tokio::time::interval(Duration::from_secs(5));
        samples.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                result = &mut stream => break result,
                result = connection.0.accept_bi() => {
                    result.context("Cell connection closed")?;
                    break Err(anyhow::anyhow!("Unexpected receiver stream"));
                },
                result = connection.0.accept_uni() => {
                    result.context("Cell connection closed")?;
                    break Err(anyhow::anyhow!("Unexpected receiver stream"));
                },
                _ = samples.tick() => {
                    resources("camera-session");
                    let path = connection.0.paths().iter().find(|path| path.is_selected()).map(|path| (path.is_relay(),path.stats()));
                    report(serde_json::json!({"type":"ailuros-camera-path",
                        "relay":path.as_ref().map(|(relay,_)|relay),
                        "rtt_ms":path.as_ref().map(|(_,stats)|stats.rtt.as_millis()),
                        "lost_packets":path.as_ref().map(|(_,stats)|stats.lost_packets),
                    }));
                }
            }
        }
    };
    result
}

fn identity(partition: EspDefaultNvsPartition) -> anyhow::Result<iroh::SecretKey> {
    let storage = EspNvs::new(partition, "lince", true)?;
    let mut bytes = [0u8; 32];
    let mut public = [0u8; 32];
    let retained_public = match storage.get_blob("iroh_public", &mut public)? {
        Some(value) => {
            ensure!(value.len() == 32, "Iroh identity recovery required");
            true
        }
        None => false,
    };
    match storage.get_blob("iroh_identity", &mut bytes)? {
        Some(value) => ensure!(value.len() == 32, "Iroh identity recovery required"),
        None => {
            ensure!(!retained_public, "Iroh identity recovery required");
            // SAFETY: The buffer is writable for exactly the supplied length.
            unsafe { esp_idf_svc::sys::esp_fill_random(bytes.as_mut_ptr().cast(), bytes.len()) };
            storage.set_blob("iroh_identity", &bytes)?;
        }
    }
    let key = iroh::SecretKey::from_bytes(&bytes);
    bytes.fill(0);
    if retained_public {
        ensure!(
            key.public().as_bytes() == &public,
            "Iroh identity recovery required"
        );
    } else {
        storage.set_blob("iroh_public", key.public().as_bytes())?;
    }
    Ok(key)
}
