use crate::{Result, configuration::Configuration};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_LINE: usize = 8192;

#[derive(Clone, Debug, Serialize)]
pub struct Port {
    pub path: String,
    pub usb_vendor: Option<u16>,
    pub usb_product: Option<u16>,
    pub serial: Option<String>,
}

pub fn ports() -> Result<Vec<Port>> {
    serialport::available_ports()
        .map_err(|error| error.to_string())
        .map(|ports| {
            ports
                .into_iter()
                .map(|port| {
                    let (usb_vendor, usb_product, serial) = match port.port_type {
                        serialport::SerialPortType::UsbPort(info) => {
                            (Some(info.vid), Some(info.pid), info.serial_number)
                        }
                        _ => (None, None, None),
                    };
                    Port {
                        path: port.port_name,
                        usb_vendor,
                        usb_product,
                        serial,
                    }
                })
                .collect()
        })
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Acquire) {
            Err("Operation cancelled".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub protocol: u32,
    pub chip: String,
    pub public_key: String,
    pub firmware: String,
    pub provisioning: bool,
    pub flash_bytes: u32,
    pub psram_bytes: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceStatus {
    pub wifi_connected: bool,
    pub phase: String,
}

impl Identity {
    pub fn validate(&self) -> Result<()> {
        if self.protocol != 1
            || self.chip != "esp32s3"
            || self.public_key.len() != 66
            || !matches!(self.public_key.get(..2), Some("02" | "03"))
            || !self
                .public_key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("This device does not report a compatible Lince firmware identity".into());
        }
        Ok(())
    }
}

pub struct Session {
    port: Box<dyn serialport::SerialPort>,
    _lease: PortLease,
}

pub(crate) struct PortLease {
    _file: std::fs::File,
}

pub(crate) fn lease(path: &str) -> Result<PortLease> {
    use sha2::{Digest, Sha384};
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path));
    let identity = canonical.to_str().ok_or("Serial path must be UTF-8")?;
    let digest: String = Sha384::digest(identity.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut directory = std::env::temp_dir();
    #[cfg(unix)]
    directory.push(format!(
        "lince-ailuros-{}",
        rustix::process::geteuid().as_raw()
    ));
    #[cfg(not(unix))]
    directory.push("lince-ailuros");
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.to_string()),
    }
    let metadata = std::fs::symlink_metadata(&directory).map_err(|e| e.to_string())?;
    if !metadata.is_dir() {
        return Err("Invalid serial lock folder".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o077 != 0 {
            return Err("Serial lock folder has unsafe permissions".into());
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(directory.join(format!("{digest}.lock")))
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|_| "Another Lince operation owns this serial device")?;
    Ok(PortLease { _file: file })
}

impl Session {
    pub fn open(path: &str) -> Result<Self> {
        if path.is_empty() || path.contains('\0') {
            return Err("Select a serial device".into());
        }
        let lease = lease(path)?;
        let mut port = serialport::new(path, 115200).dtr_on_open(false).timeout(Duration::from_millis(100)).open()
            .map_err(|error| format!("Cannot open selected serial device: {error}. Check serial permissions and close other monitors."))?;
        port.write_request_to_send(false)
            .map_err(|_| "Could not release the ESP reset line")?;
        Ok(Self {
            port,
            _lease: lease,
        })
    }

    pub fn identity(&mut self, cancellation: &Cancellation) -> Result<Identity> {
        let data = exchange(&mut self.port, "hello", Value::Null, cancellation)?;
        let identity: Identity =
            serde_json::from_value(data).map_err(|_| "Device returned an invalid identity")?;
        identity.validate()?;
        Ok(identity)
    }

    pub fn status(
        &mut self,
        expected_public_key: &str,
        cancellation: &Cancellation,
    ) -> Result<DeviceStatus> {
        if self.identity(cancellation)?.public_key != expected_public_key {
            return Err("Connected device differs from the enrolled device".into());
        }
        let data = exchange(&mut self.port, "status", Value::Null, cancellation)?;
        let status: DeviceStatus =
            serde_json::from_value(data).map_err(|_| "Invalid device status")?;
        if status.phase.len() > 128 || status.phase.chars().any(char::is_control) {
            return Err("Invalid device status".into());
        }
        Ok(status)
    }

    pub fn configure(
        &mut self,
        configuration: &Configuration,
        expected_public_key: &str,
        cancellation: &Cancellation,
    ) -> Result<()> {
        configuration.validate()?;
        let identity = self.identity(cancellation)?;
        if identity.public_key != expected_public_key {
            return Err(
                "Device key changed; enroll the connected device before provisioning".into(),
            );
        }
        if let crate::configuration::Workflow::Camera { width, .. } = &configuration.workflow {
            if identity.psram_bytes == 0 && *width > 320 {
                return Err("Installed firmware reports no PSRAM; choose at most 320×240 or compile a verified PSRAM configuration".into());
            }
        }
        if !identity.provisioning {
            return Err("Hold BOOT for two seconds on the running ESP, then release it to enter provisioning mode".into());
        }
        let mut data =
            serde_json::to_value(configuration).map_err(|_| "Cannot encode configuration")?;
        data["setup"]["key_epoch"] = Value::String(configuration.setup.key_epoch.to_string());
        data["server"] = Value::String(
            url::Url::parse(&configuration.server)
                .map_err(|_| "Invalid destination")?
                .origin()
                .ascii_serialization(),
        );
        let bootstrap = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Host clock is incorrect")?
            .as_secs();
        if !(1_700_000_000..=4_102_444_800).contains(&bootstrap) {
            return Err("Correct the host clock before provisioning".into());
        }
        data["bootstrap_time"] = Value::from(bootstrap);
        if serde_json::to_vec(&data)
            .map_err(|_| "Cannot encode provisioning request")?
            .len()
            >= 7168
        {
            return Err("Provisioned configuration exceeds the device budget".into());
        }
        let result = exchange(&mut self.port, "configure", data, cancellation)?;
        if result["configured"] != true {
            return Err("Device did not confirm configuration storage".into());
        }
        Ok(())
    }
}

fn exchange(
    port: &mut (impl Read + Write),
    operation: &str,
    data: Value,
    cancellation: &Cancellation,
) -> Result<Value> {
    exchange_until(
        port,
        operation,
        data,
        cancellation,
        Instant::now() + Duration::from_secs(8),
    )
}

fn exchange_until(
    port: &mut (impl Read + Write),
    operation: &str,
    data: Value,
    cancellation: &Cancellation,
    deadline: Instant,
) -> Result<Value> {
    cancellation.check()?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let mut bytes = serde_json::to_vec(&json!({"id":id,"operation":operation,"data":data}))
        .map_err(|_| "Cannot encode provisioning request")?;
    if bytes.len() >= MAX_LINE {
        return Err("Provisioning request too large".into());
    }
    bytes.push(b'\n');
    port.write_all(&bytes)
        .and_then(|()| port.flush())
        .map_err(|_| "Could not send provisioning request")?;
    let mut line = Vec::new();
    let mut discarded = 0_usize;
    loop {
        cancellation.check()?;
        if Instant::now() >= deadline {
            return Err(
                "Device response timed out; install Lince firmware and check provisioning mode"
                    .into(),
            );
        }
        let mut byte = [0];
        match port.read(&mut byte) {
            Ok(0) => return Err("Device disconnected".into()),
            Ok(_) if byte[0] == b'\n' => {
                let response = serde_json::from_slice::<Value>(&line);
                line.clear();
                let Ok(response) = response else {
                    continue;
                };
                if response["id"] != id {
                    continue;
                }
                if response["ok"] != true {
                    return Err("Device refused provisioning; check its physical setup mode and configuration".into());
                }
                return Ok(response["data"].clone());
            }
            Ok(_) => {
                line.push(byte[0]);
                if line.len() > MAX_LINE {
                    return Err("Device sent an oversized response".into());
                }
                discarded += 1;
                if discarded > 64 * 1024 {
                    return Err("Device output exceeded the provisioning budget".into());
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Err("Device connection failed".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selected_port_has_one_owner_across_jobs() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("device");
        std::fs::write(&path, []).unwrap();
        let selected = path.to_str().unwrap();
        let first = lease(selected).unwrap();
        assert!(lease(selected).is_err());
        drop(first);
        assert!(lease(selected).is_ok());
    }

    struct Device {
        input: std::io::Cursor<Vec<u8>>,
        output: Vec<u8>,
        response: String,
    }

    impl Read for Device {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buffer)
        }
    }

    impl Write for Device {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.output.extend_from_slice(buffer);
            if buffer.ends_with(b"\n") {
                let request: Value = serde_json::from_slice(&self.output).unwrap();
                self.input = std::io::Cursor::new(
                    format!(
                        "boot log\n{}\n",
                        json!({"id":request["id"], "ok":true, "data":{"reply":self.response}})
                    )
                    .into_bytes(),
                );
            }
            Ok(buffer.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn framed_requests_match_responses_and_ignore_boot_logs() {
        let mut device = Device {
            input: std::io::Cursor::new(Vec::new()),
            output: Vec::new(),
            response: "ready".into(),
        };
        assert_eq!(
            exchange(&mut device, "hello", Value::Null, &Cancellation::default()).unwrap()["reply"],
            "ready"
        );
        let request: Value = serde_json::from_slice(&device.output).unwrap();
        assert_eq!(request["operation"], "hello");
        assert!(request["id"].as_str().unwrap().len() == 32);
    }

    #[test]
    fn cancellation_prevents_device_writes_and_error_messages_do_not_echo_secrets() {
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let mut device = Device {
            input: std::io::Cursor::new(Vec::new()),
            output: Vec::new(),
            response: String::new(),
        };
        assert!(
            exchange(
                &mut device,
                "configure",
                json!({"password":"secret"}),
                &cancellation
            )
            .is_err()
        );
        assert!(device.output.is_empty());
        let mut disconnected = std::io::Cursor::new(Vec::new());
        let error = exchange(
            &mut disconnected,
            "configure",
            json!({"password":"secret"}),
            &Cancellation::default(),
        )
        .unwrap_err();
        assert!(!error.contains("secret"));
    }

    struct Fault {
        read: std::io::ErrorKind,
        write: Option<std::io::ErrorKind>,
        cancellation: Option<Cancellation>,
    }

    impl Read for Fault {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            if let Some(cancellation) = &self.cancellation {
                cancellation.cancel();
            }
            Err(std::io::Error::new(self.read, "secret"))
        }
    }

    impl Write for Fault {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            match self.write {
                Some(kind) => Err(std::io::Error::new(kind, "secret")),
                None => Ok(bytes.len()),
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn serial_timeout_unplug_permissions_and_active_cancellation_are_redacted() {
        use std::io::ErrorKind;
        let cancellation = Cancellation::default();
        let mut port = Fault {
            read: ErrorKind::TimedOut,
            write: None,
            cancellation: None,
        };
        let error = exchange_until(
            &mut port,
            "configure",
            json!({"password":"secret"}),
            &cancellation,
            Instant::now(),
        )
        .unwrap_err();
        assert!(error.contains("timed out"));
        port.read = ErrorKind::BrokenPipe;
        assert_eq!(
            exchange(&mut port, "hello", Value::Null, &cancellation).unwrap_err(),
            "Device connection failed"
        );
        port.write = Some(ErrorKind::PermissionDenied);
        assert_eq!(
            exchange(
                &mut port,
                "configure",
                json!({"password":"secret"}),
                &cancellation
            )
            .unwrap_err(),
            "Could not send provisioning request"
        );
        port.write = None;
        port.read = ErrorKind::TimedOut;
        port.cancellation = Some(cancellation.clone());
        assert_eq!(
            exchange(&mut port, "hello", Value::Null, &cancellation).unwrap_err(),
            "Operation cancelled"
        );
    }
}
