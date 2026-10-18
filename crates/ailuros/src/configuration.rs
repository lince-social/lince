use crate::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub audience: String,
    pub device: String,
    pub signal: String,
    pub key_epoch: i64,
}

fn uid(value: &str) -> bool {
    value.strip_prefix("r_").is_some_and(|body| {
        body.len() == 26
            && body
                .bytes()
                .all(|byte| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&byte))
    })
}

impl Setup {
    pub fn validate(&self) -> Result<()> {
        if self.audience.len() != 64
            || !self
                .audience
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !uid(&self.device)
            || !uid(&self.signal)
            || self.key_epoch <= 0
        {
            return Err("Choose a confirmed device and Signal from the destination Cell".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraPins {
    pub data: [i32; 8],
    pub xclk: i32,
    pub pclk: i32,
    pub vsync: i32,
    pub href: i32,
    pub sda: i32,
    pub scl: i32,
    pub pwdn: i32,
    pub reset: i32,
}

impl CameraPins {
    pub fn validate(&self) -> Result<()> {
        let mut used = std::collections::BTreeSet::new();
        for pin in self.data.iter().copied().chain([
            self.xclk, self.pclk, self.vsync, self.href, self.sda, self.scl,
        ]) {
            if !(1..=48).contains(&pin)
                || (22..=25).contains(&pin)
                || [
                    19, 20, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 43, 44, 45, 46,
                ]
                .contains(&pin)
                || !used.insert(pin)
            {
                return Err("Camera pins conflict or use a reserved ESP32-S3 flash, PSRAM, serial, USB or boot pin".into());
            }
        }
        for pin in [self.pwdn, self.reset] {
            if pin == -1 {
                continue;
            }
            if !(1..=48).contains(&pin)
                || (22..=46).contains(&pin)
                || [19, 20].contains(&pin)
                || !used.insert(pin)
            {
                return Err("Invalid camera power/reset pin".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Workflow {
    Camera {
        pins: CameraPins,
        width: u32,
        height: u32,
        fps: u32,
        quality: u32,
    },
    Scalar {
        value: String,
        interval_seconds: u32,
    },
    Custom {
        name: String,
        output: String,
        settings: serde_json::Value,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub version: u32,
    pub ssid: String,
    pub password: String,
    pub server: String,
    pub ca_pem: String,
    pub time_server: String,
    pub setup: Setup,
    pub workflow: Workflow,
}

impl Configuration {
    pub fn validate(&self) -> Result<()> {
        self.setup.validate()?;
        if self.version != 1
            || self.ssid.is_empty()
            || self.ssid.len() > 32
            || self.ssid.chars().any(char::is_control)
        {
            return Err("Choose a Wi-Fi SSID of 1–32 bytes and configuration version 1".into());
        }
        if !(self.password.is_empty()
            || (8..=63).contains(&self.password.len())
            || (self.password.len() == 64 && self.password.bytes().all(|b| b.is_ascii_hexdigit())))
            || self.password.contains('\0')
        {
            return Err("Use an open network or a valid WPA password".into());
        }
        if self.server.len() > 512 || self.server.chars().any(char::is_control) {
            return Err("Invalid destination address".into());
        }
        let url =
            url::Url::parse(&self.server).map_err(|_| "Use the destination Cell's HTTPS origin")?;
        if url.scheme() != "https"
            || url.origin().ascii_serialization().len() > 512
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Use an HTTPS origin without credentials, path, query or fragment".into());
        }
        if self.ca_pem.len() > 4096
            || (!self.ca_pem.is_empty()
                && (!self.ca_pem.starts_with("-----BEGIN CERTIFICATE-----")
                    || !self
                        .ca_pem
                        .trim_end()
                        .ends_with("-----END CERTIFICATE-----")))
        {
            return Err("Provide a PEM CA certificate or use the firmware CA bundle".into());
        }
        if self.time_server.is_empty()
            || self.time_server.len() > 128
            || !self
                .time_server
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
        {
            return Err("Choose a time server hostname or IP address".into());
        }
        match &self.workflow {
            Workflow::Camera {
                pins,
                width,
                height,
                fps,
                quality,
            } => {
                pins.validate()?;
                if !matches!(
                    (*width, *height),
                    (160, 120) | (320, 240) | (640, 480) | (800, 600) | (1280, 720)
                ) || !(1..=15).contains(fps)
                    || !(10..=63).contains(quality)
                {
                    return Err(
                        "Choose a supported camera resolution, 1–15 FPS and JPEG quality 10–63"
                            .into(),
                    );
                }
            }
            Workflow::Scalar {
                value,
                interval_seconds,
            } => {
                if !canonical_decimal(value) || !(1..=86400).contains(interval_seconds) {
                    return Err("Choose a canonical decimal and a 1–86400 second interval".into());
                }
            }
            Workflow::Custom {
                name,
                output,
                settings,
            } => {
                if name.is_empty()
                    || name.len() > 64
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
                    || !matches!(output.as_str(), "scalar" | "jpeg")
                    || !settings.is_object()
                    || serde_json::to_vec(settings)
                        .map_err(|_| "Invalid custom settings")?
                        .len()
                        > 2048
                {
                    return Err(
                        "Choose a custom workflow name and a settings object of at most 2048 bytes"
                            .into(),
                    );
                }
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| "Cannot serialize configuration")?
            .len()
            > 7040
        {
            return Err("Configuration exceeds the provisioning budget".into());
        }
        Ok(())
    }
}

fn canonical_decimal(value: &str) -> bool {
    if value.is_empty() || value.len() > 64 {
        return false;
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    let mut parts = digits.split('.');
    let integer = parts.next().unwrap_or_default();
    if integer.is_empty()
        || !integer.bytes().all(|b| b.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
    {
        return false;
    }
    if let Some(fraction) = parts.next() {
        if fraction.is_empty()
            || fraction.len() > 18
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return false;
        }
    }
    let Ok(mantissa) = value.replace('.', "").parse::<i128>() else {
        return false;
    };
    parts.next().is_none() && (!value.starts_with('-') || mantissa != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Configuration {
        Configuration {
            version: 1,
            ssid: "Camera Wi-Fi".into(),
            password: "password-for-test".into(),
            server: "https://cell.example".into(),
            ca_pem: String::new(),
            time_server: "pool.ntp.org".into(),
            setup: Setup {
                audience: "00".repeat(32),
                device: "r_00000000000000000000000001".into(),
                signal: "r_00000000000000000000000002".into(),
                key_epoch: 1,
            },
            workflow: Workflow::Scalar {
                value: "12.5".into(),
                interval_seconds: 60,
            },
        }
    }

    #[test]
    fn refuses_untrusted_destinations_and_unbounded_settings() {
        let mut config = config();
        config.validate().unwrap();
        for server in [
            "http://cell.example",
            "https://user:password@cell.example",
            "https://cell.example/path",
            "https://cell.example/?token=secret",
        ] {
            config.server = server.into();
            assert!(config.validate().is_err());
        }
        config.server = "https://cell.example".into();
        config.password = "short".into();
        assert!(config.validate().is_err());
        config.password.clear();
        config.setup.key_epoch = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn decimal_and_camera_resource_constraints_are_explicit() {
        for value in ["0", "-1", "12.5", "0.01", "1.0", "0.00"] {
            assert!(canonical_decimal(value));
        }
        for value in [
            "-0",
            "01",
            "+1",
            "1e2",
            "-0.00",
            "1\n2",
            ".2",
            "0.0000000000000000001",
            "170141183460469231731687303715884105728",
        ] {
            assert!(!canonical_decimal(value));
        }
        let mut pins = CameraPins {
            data: [11, 9, 8, 10, 12, 18, 17, 16],
            xclk: 15,
            pclk: 13,
            vsync: 6,
            href: 7,
            sda: 4,
            scl: 5,
            pwdn: -1,
            reset: -1,
        };
        pins.validate().unwrap();
        pins.xclk = 11;
        assert!(pins.validate().is_err());
        pins.xclk = 33;
        assert!(pins.validate().is_err());
    }

    #[test]
    fn custom_workflows_share_identity_and_bound_their_runtime_settings() {
        let mut config = config();
        config.workflow = Workflow::Custom {
            name: "scalar-demo".into(),
            output: "scalar".into(),
            settings: serde_json::json!({"value":"12.5","interval_seconds":60}),
        };
        config.validate().unwrap();
        config.workflow = Workflow::Custom {
            name: "custom-camera".into(),
            output: "jpeg".into(),
            settings: serde_json::json!({"source":"sensor"}),
        };
        config.validate().unwrap();
        config.workflow = Workflow::Custom {
            name: "bad\nname".into(),
            output: "jpeg".into(),
            settings: serde_json::json!({}),
        };
        assert!(config.validate().is_err());
        config.workflow = Workflow::Custom {
            name: "large".into(),
            output: "scalar".into(),
            settings: serde_json::json!({"payload":"x".repeat(2049)}),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn firmware_decimal_fixtures_match_host_validation() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../firmware/esp-idf/fixtures/decimals.json"))
                .unwrap();
        for case in cases {
            assert_eq!(
                canonical_decimal(case["value"].as_str().unwrap()),
                case["accepted"].as_bool().unwrap(),
                "{case}"
            );
        }
    }
}
