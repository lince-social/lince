use crate::{Result, configuration::CameraPins};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardProfile {
    pub version: u32,
    pub name: String,
    pub chip: String,
    pub minimum_flash_bytes: u32,
    pub psram: Option<Psram>,
    pub camera: Option<Camera>,
    pub reference: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Psram {
    pub mode: PsramMode,
    pub speed_mhz: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PsramMode {
    Quad,
    Octal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Camera {
    pub sensor: String,
    pub pins: CameraPins,
    pub xclk_hz: u32,
}

pub const DEFAULT_BOARD: &str = "esp32s3";

pub fn catalog() -> Vec<(&'static str, &'static str)> {
    vec![
        ("esp32s3", include_str!("../boards/esp32s3.json")),
        (
            "goouuu-esp32s3-cam-v1.5",
            include_str!("../boards/goouuu-esp32s3-cam-v1.5.json"),
        ),
    ]
}

pub fn builtin(id: &str) -> Result<BoardProfile> {
    let (_, json) = catalog()
        .into_iter()
        .find(|(name, _)| *name == id)
        .ok_or("Unknown board profile; list the available profiles first")?;
    let profile: BoardProfile = serde_json::from_str(json).map_err(|e| e.to_string())?;
    profile.validate()?;
    Ok(profile)
}

impl BoardProfile {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1
            || self.name.trim().is_empty()
            || self.name.len() > 128
            || self.chip != "esp32s3"
            || ![4, 8, 16, 32]
                .map(|size| size * 1024 * 1024)
                .contains(&self.minimum_flash_bytes)
            || self.reference.len() > 2048
            || self.reference.chars().any(char::is_control)
        {
            return Err("Choose an ESP32-S3 profile with 4, 8, 16 or 32 MiB minimum physical flash; installation retains the protected 4 MiB application layout".into());
        }
        if let Some(psram) = &self.psram
            && ![40, 80].contains(&psram.speed_mhz)
        {
            return Err("Choose a supported PSRAM clock: 40 or 80 MHz".into());
        }
        if let Some(camera) = &self.camera {
            camera.pins.validate()?;
            if !(6_000_000..=24_000_000).contains(&camera.xclk_hz)
                || camera.sensor.is_empty()
                || camera.sensor.len() > 64
                || !camera
                    .sensor
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            {
                return Err(
                    "Specify the camera sensor and a verified 6–24 MHz external clock".into(),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_catalog_separates_unconfigured_hardware_from_the_verified_camera() {
        let generic = builtin(DEFAULT_BOARD).unwrap();
        assert!(generic.camera.is_none());
        assert!(generic.psram.is_none());
        let profile = builtin("goouuu-esp32s3-cam-v1.5").unwrap();
        assert_eq!(profile.name, "GOOUUU ESP32-S3-CAM V1.5");
        assert_eq!(profile.minimum_flash_bytes, 16 * 1024 * 1024);
        let camera = profile.camera.as_ref().unwrap();
        assert_eq!(camera.xclk_hz, 8_000_000);
        let pins = &camera.pins;
        assert_eq!(pins.data, [11, 9, 8, 10, 12, 18, 17, 16]);
        assert_eq!(pins.xclk, 15);
        let mut bad = profile;
        bad.chip = "esp32".into();
        assert!(bad.validate().is_err());
        assert!(builtin("../private-board").is_err());
    }

    #[test]
    fn custom_profiles_reject_invalid_memory_and_camera_settings() {
        let mut profile = builtin("goouuu-esp32s3-cam-v1.5").unwrap();
        profile.minimum_flash_bytes = 2 * 1024 * 1024;
        assert!(profile.validate().is_err());
        profile.minimum_flash_bytes = 8 * 1024 * 1024;
        profile.psram.as_mut().unwrap().speed_mhz = 120;
        assert!(profile.validate().is_err());
        profile.psram.as_mut().unwrap().speed_mhz = 40;
        profile.camera.as_mut().unwrap().xclk_hz = 0;
        assert!(profile.validate().is_err());
        profile.camera.as_mut().unwrap().xclk_hz = 16_000_000;
        profile.validate().unwrap();
        profile.camera.as_mut().unwrap().pins.sda = 11;
        assert!(profile.validate().is_err());
    }
}
