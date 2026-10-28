use crate::launch::Description;
use lince_interface::host::DesktopAction;
pub(crate) use lince_interface::host::input::KeyboardSettings as Keyboard;
use serde::Deserialize;
use std::{io, path::Path};

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Commands {
    pub outputs: std::collections::BTreeMap<String, OutputPreference>,
    pub keyboard: Keyboard,
    pub pointer: lince_interface::host::input::PointerSettings,
    pub devices: lince_interface::host::input::AbsolutePreferences,
    pub external: lince_interface::host::external::Settings,
    pub terminal: Option<Description>,
    pub launcher: Option<Description>,
    pub lock: Option<Description>,
    pub ready: Option<Description>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct OutputPreference {
    pub enabled: bool,
    pub scale: f64,
    pub position: Option<[i32; 2]>,
    pub mode: Option<[u32; 2]>,
    pub refresh_mhz: Option<i32>,
    pub magnification: f64,
    pub rotation: lince_interface::host::mapping::QuarterTurn,
    pub adaptive_sync: bool,
    pub color: lince_interface::host::color::ColorMode,
}

impl Default for OutputPreference {
    fn default() -> Self {
        Self {
            enabled: true,
            scale: 1.0,
            position: None,
            mode: None,
            refresh_mhz: None,
            magnification: 1.0,
            rotation: Default::default(),
            adaptive_sync: false,
            color: Default::default(),
        }
    }
}

impl Commands {
    pub fn load(path: &Path) -> io::Result<Self> {
        Self::parse(&Self::read(path)?.unwrap_or_else(|| b"{}".to_vec()))
    }

    fn read(path: &Path) -> io::Result<Option<Vec<u8>>> {
        use std::io::Read;
        let mut bytes = Vec::new();
        match std::fs::File::open(path) {
            Ok(file) => {
                file.take(65537).read_to_end(&mut bytes)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        }
        if bytes.len() > 65536 {
            return Err(io::Error::other(
                "Park command configuration exceeds 64 KiB",
            ));
        }
        Ok(Some(bytes))
    }

    fn parse(bytes: &[u8]) -> io::Result<Self> {
        let commands: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        if commands.outputs.len() > 16
            || commands.outputs.iter().any(|(name, output)| {
                name.is_empty()
                    || name.len() > 512
                    || !output.scale.is_finite()
                    || !(0.5..=4.0).contains(&output.scale)
                    || !lince_interface::host::magnification::valid(output.magnification)
                    || (output.scale * 120.0 - (output.scale * 120.0).round()).abs() > 0.000001
                    || output.position.is_some_and(|position| {
                        position.iter().any(|value| !(0..=32768).contains(value))
                    })
                    || output
                        .mode
                        .is_some_and(|mode| mode.iter().any(|value| !(1..=16384).contains(value)))
                    || output
                        .refresh_mhz
                        .is_some_and(|refresh| !(1000..=1_000_000).contains(&refresh))
            })
        {
            return Err(io::Error::other("Invalid local output preferences"));
        }
        commands.keyboard.validate().map_err(io::Error::other)?;
        commands.pointer.validate().map_err(io::Error::other)?;
        lince_interface::host::input::validate_absolute(&commands.devices).map_err(io::Error::other)?;
        commands.external.validate().map_err(io::Error::other)?;
        for command in [
            &commands.terminal,
            &commands.launcher,
            &commands.lock,
            &commands.ready,
        ]
        .into_iter()
        .flatten()
        {
            if !command.valid() {
                return Err(io::Error::other("Invalid Park command configuration"));
            }
        }
        Ok(commands)
    }

    pub fn save_outputs(
        path: &Path,
        heads: &[crate::server::OutputHead],
    ) -> io::Result<Option<String>> {
        Self::save_outputs_with_sync(path, heads, |parent| {
            std::fs::File::open(parent)?.sync_all()
        })
    }

    fn save_outputs_with_sync(
        path: &Path,
        heads: &[crate::server::OutputHead],
        sync_directory: impl FnOnce(&Path) -> io::Result<()>,
    ) -> io::Result<Option<String>> {
        crate::server::validate_output_catalog(heads)?;
        Self::update(path, "Monitor settings", sync_directory, |document| {
            Self::merge_outputs(document, heads)
        })
    }

    pub fn save_magnification(path: &Path, name: &str, factor: f64, expected: f64) -> io::Result<Option<String>> {
        if name.is_empty() || name.len() > 512 || !lince_interface::host::magnification::valid(factor)
            || !lince_interface::host::magnification::valid(expected)
        { return Err(io::Error::other("Invalid output magnification settings.")); }
        Self::update(path, "Display magnification", |parent| std::fs::File::open(parent)?.sync_all(), |document| {
            let current = Self::parse(&serde_json::to_vec(document).map_err(io::Error::other)?)?;
            if current.outputs.get(name).map_or(1.0, |output| output.magnification) != expected {
                return Err(io::Error::other("Output magnification changed outside Park; reload before saving."));
            }
            let output = document.as_object_mut().ok_or_else(|| io::Error::other("Park settings must be an object."))?
                .entry("outputs").or_insert_with(|| serde_json::json!({})).as_object_mut()
                .ok_or_else(|| io::Error::other("Park output settings must be an object."))?
                .entry(name.to_owned()).or_insert_with(|| serde_json::json!({})).as_object_mut()
                .ok_or_else(|| io::Error::other("Invalid saved output settings."))?;
            output.insert("magnification".into(), serde_json::json!(factor));
            Ok(())
        })
    }

    pub fn save_keyboard(
        path: &Path,
        settings: &Keyboard,
        expected: &Keyboard,
    ) -> io::Result<Option<String>> {
        settings.validate().map_err(io::Error::other)?;
        Self::update(
            path,
            "Keyboard settings",
            |parent| std::fs::File::open(parent)?.sync_all(),
            |document| {
                let current =
                    Self::parse(&serde_json::to_vec(document).map_err(io::Error::other)?)?;
                if &current.keyboard != expected {
                    return Err(io::Error::other(
                        "Keyboard configuration changed outside Park; reload it before saving.",
                    ));
                }
                document
                    .as_object_mut()
                    .ok_or_else(|| io::Error::other("Park settings must be an object"))?
                    .insert(
                        "keyboard".into(),
                        serde_json::to_value(settings).map_err(io::Error::other)?,
                    );
                Ok(())
            },
        )
    }

    pub fn save_external(
        path: &Path,
        settings: &lince_interface::host::external::Settings,
        expected: &lince_interface::host::external::Settings,
    ) -> io::Result<Option<String>> {
        settings.validate().map_err(io::Error::other)?;
        Self::update(
            path,
            "External commands",
            |parent| std::fs::File::open(parent)?.sync_all(),
            |document| {
                let current =
                    Self::parse(&serde_json::to_vec(document).map_err(io::Error::other)?)?;
                if &current.external != expected {
                    return Err(io::Error::other(
                        "External commands changed outside Park; reload before saving.",
                    ));
                }
                document
                    .as_object_mut()
                    .ok_or_else(|| io::Error::other("Park settings must be an object"))?
                    .insert(
                        "external".into(),
                        serde_json::to_value(settings).map_err(io::Error::other)?,
                    );
                Ok(())
            },
        )
    }

    #[cfg(any(feature = "session", test))]
    pub fn save_pointer(
        path: &Path,
        settings: &lince_interface::host::input::PointerSettings,
        expected: &lince_interface::host::input::PointerSettings,
    ) -> io::Result<Option<String>> {
        settings.validate().map_err(io::Error::other)?;
        Self::update(
            path,
            "Pointer settings",
            |parent| std::fs::File::open(parent)?.sync_all(),
            |document| {
                let current =
                    Self::parse(&serde_json::to_vec(document).map_err(io::Error::other)?)?;
                if &current.pointer != expected {
                    return Err(io::Error::other(
                        "Pointer configuration changed outside Park; reload it before saving.",
                    ));
                }
                document
                    .as_object_mut()
                    .ok_or_else(|| io::Error::other("Park settings must be an object"))?
                    .insert(
                        "pointer".into(),
                        serde_json::to_value(settings).map_err(io::Error::other)?,
                    );
                Ok(())
            },
        )
    }

    #[cfg(any(feature = "session", test))]
    pub fn save_devices(
        path: &Path,
        settings: &lince_interface::host::input::AbsolutePreferences,
        expected: &lince_interface::host::input::AbsolutePreferences,
    ) -> io::Result<Option<String>> {
        lince_interface::host::input::validate_absolute(settings).map_err(io::Error::other)?;
        Self::update(path, "Touch and tablet settings", |parent| std::fs::File::open(parent)?.sync_all(), |document| {
            let current = Self::parse(&serde_json::to_vec(document).map_err(io::Error::other)?)?;
            if &current.devices != expected {
                return Err(io::Error::other("Device settings changed outside Park; reload before saving."));
            }
            document.as_object_mut().ok_or_else(|| io::Error::other("Park settings must be an object"))?
                .insert("devices".into(), serde_json::to_value(settings).map_err(io::Error::other)?);
            Ok(())
        })
    }

    fn update(
        path: &Path,
        caption: &str,
        sync_directory: impl FnOnce(&Path) -> io::Result<()>,
        edit: impl FnOnce(&mut serde_json::Value) -> io::Result<()>,
    ) -> io::Result<Option<String>> {
        use std::{io::Write, os::unix::fs::PermissionsExt};
        let previous = Self::read(path)?;
        let bytes = previous.as_deref().unwrap_or(b"{}");
        Self::parse(bytes)?;
        let mut document: serde_json::Value =
            serde_json::from_slice(bytes).map_err(io::Error::other)?;
        edit(&mut document)?;
        let mut bytes = serde_json::to_vec_pretty(&document).map_err(io::Error::other)?;
        bytes.push(b'\n');
        if bytes.len() > 65536 {
            return Err(io::Error::other(
                "Park command configuration exceeds 64 KiB",
            ));
        }
        Self::parse(&bytes)?;
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("The Park configuration has no parent directory"))?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        if Self::read(path)? != previous {
            return Err(io::Error::other(
                "Park configuration changed while saving; retry to merge the latest settings",
            ));
        }
        file.persist(path).map_err(io::Error::other)?;
        Ok(sync_directory(parent).err().map(|error| {
            format!(
                "{caption} were saved, but directory durability could not be confirmed: {error}"
            )
        }))
    }

    fn merge_outputs(
        document: &mut serde_json::Value,
        heads: &[crate::server::OutputHead],
    ) -> io::Result<()> {
        let outputs = document
            .as_object_mut()
            .ok_or_else(|| io::Error::other("Park settings must be an object"))?
            .entry("outputs")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| io::Error::other("Park output settings must be an object"))?;
        for head in heads {
            let entry = outputs
                .entry(head.name.clone())
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
                .ok_or_else(|| io::Error::other("Invalid saved output settings"))?;
            entry.insert("enabled".into(), head.current.is_some().into());
            entry.insert("rotation".into(), serde_json::to_value(head.rotation).map_err(io::Error::other)?);
            entry.insert("adaptive_sync".into(), head.adaptive_sync.into());
            entry.insert("color".into(), serde_json::to_value(head.color).map_err(io::Error::other)?);
            if let Some(settings) = head.settings() {
                entry.insert("scale".into(), settings.scale.into());
                entry.insert("position".into(), serde_json::json!(settings.position));
                entry.insert("mode".into(), serde_json::json!(settings.mode.size));
                entry.insert(
                    "refresh_mhz".into(),
                    if settings.mode.refresh_mhz == 0 {
                        serde_json::Value::Null
                    } else {
                        settings.mode.refresh_mhz.into()
                    },
                );
            }
        }
        Ok(())
    }

    pub fn command(&self, action: DesktopAction) -> io::Result<Description> {
        let command = match action {
            DesktopAction::OpenTerminal => self.terminal.clone().or_else(|| {
                Some(Description {
                    program: "xdg-terminal-exec".into(),
                    arguments: Vec::new(),
                    directory: None,
                    capabilities: Vec::new(),
                })
            }),
            DesktopAction::OpenLauncher => self.launcher.clone(),
            DesktopAction::Lock => self.lock.clone(),
            DesktopAction::External(action) => Some(
                self.external
                    .command(action, false)
                    .map_err(io::Error::other)?
                    .clone(),
            ),
            _ => return Err(io::Error::other("This action is not an external command")),
        };
        command.ok_or_else(|| {
            io::Error::other("Choose this command in the local park.json configuration")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_saves_merge_preserve_disconnected_intent_and_reject_stale_or_invalid_candidates() {
        use lince_interface::host::input::AbsolutePreference;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        let original = serde_json::json!({
            "keyboard": {"layout": "br"}, "pointer": {"tap": true},
            "outputs": {"DP-1": {"scale": 1.25}},
            "terminal": {"program": "chosen-terminal"},
            "devices": {"disconnected": {"output": "HDMI-A-1"}}
        });
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let before = Commands::load(&path).unwrap().devices;
        let mut settings = before.clone();
        settings.insert("stable-touch".into(), AbsolutePreference { output: Some("DP-1".into()), calibration: Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]), ..Default::default() });
        Commands::save_devices(&path, &settings, &before).unwrap();
        assert_eq!(Commands::load(&path).unwrap().devices, settings);
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for key in ["keyboard", "pointer", "outputs", "terminal"] { assert_eq!(saved[key], original[key]); }
        let bytes = std::fs::read(&path).unwrap();
        assert!(Commands::save_devices(&path, &before, &before).is_err());
        settings.get_mut("stable-touch").unwrap().calibration = Some([0.0; 6]);
        assert!(Commands::save_devices(&path, &settings, &Commands::load(&path).unwrap().devices).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn pointer_saves_merge_with_keyboard_outputs_and_providers_and_reject_stale_or_invalid_values()
    {
        use lince_interface::host::input::PointerSettings;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        let original = serde_json::json!({
            "terminal": { "program": "chosen-terminal", "arguments": ["two words"] },
            "outputs": { "DP-1": { "scale": 1.5 } },
            "keyboard": { "layout": "br", "repeat_rate": 35 },
            "pointer": { "tap": false }
        });
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let previous = Commands::load(&path).unwrap().pointer;
        let requested = PointerSettings {
            tap: Some(true),
            acceleration: Some(0.4),
            ..Default::default()
        };
        assert!(
            Commands::save_pointer(&path, &requested, &previous)
                .unwrap()
                .is_none()
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for field in ["terminal", "keyboard", "outputs"] {
            assert_eq!(saved[field], original[field]);
        }
        assert_eq!(Commands::load(&path).unwrap().pointer, requested);
        let keyboard = Commands::load(&path).unwrap().keyboard;
        Commands::save_keyboard(
            &path,
            &Keyboard {
                layout: "us".into(),
                ..keyboard.clone()
            },
            &keyboard,
        )
        .unwrap();
        assert_eq!(Commands::load(&path).unwrap().pointer, requested);
        let before = std::fs::read(&path).unwrap();
        assert!(Commands::save_pointer(&path, &previous, &previous).is_err());
        assert!(
            Commands::save_pointer(
                &path,
                &PointerSettings {
                    acceleration: Some(f64::NAN),
                    ..requested.clone()
                },
                &requested
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        std::fs::write(&path, br#"{"pointer":{"tap":true,"accel":0.3}}"#).unwrap();
        assert!(Commands::load(&path).is_err());
    }

    #[test]
    fn keyboard_saves_preserve_other_settings_and_reject_a_stale_keyboard_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        let document = serde_json::json!({
            "terminal": { "program": "chosen-terminal", "arguments": ["$(literal)", "two words"] },
            "outputs": { "DP-1": { "scale": 1.5 } },
            "keyboard": { "layout": "us" }
        });
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        let original = Commands::load(&path).unwrap().keyboard;
        let settings = Keyboard {
            layout: "br".into(),
            repeat_rate: 35,
            repeat_delay_ms: 300,
            ..Default::default()
        };
        assert!(
            Commands::save_keyboard(&path, &settings, &original)
                .unwrap()
                .is_none()
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["terminal"], document["terminal"]);
        assert_eq!(saved["outputs"], document["outputs"]);
        assert_eq!(Commands::load(&path).unwrap().keyboard, settings);
        let bytes = std::fs::read(&path).unwrap();
        assert!(Commands::save_keyboard(&path, &original, &original).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn confirmed_outputs_merge_atomically_without_replacing_commands_keyboard_or_disconnected_preferences()
     {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        let initial = serde_json::json!({
            "terminal": { "program": "chosen-terminal", "arguments": ["two words", "$(literal)"], "directory": null },
            "keyboard": { "repeat_rate": 35, "repeat_delay_ms": 300 },
            "outputs": { "DP-9": { "enabled": false, "mode": [1920, 1080], "scale": 1.25, "adaptive_sync": true, "magnification": 3.0 } }
        });
        std::fs::write(&path, serde_json::to_vec(&initial).unwrap()).unwrap();
        let head = crate::server::OutputHead { color: lince_interface::host::color::ColorMode::Hdr10, adaptive_sync: true, rotation: lince_interface::host::mapping::QuarterTurn::Clockwise90,
            id: lince_interface::host::OutputId(1),
            name: "DP-1".into(),
            modes: vec![crate::server::OutputMode {
                size: [2560, 1440],
                refresh_mhz: 144000,
                preferred: true,
            }],
            current: Some(0),
            position: [0, 0],
            scale: 1.5,
        };
        assert!(
            Commands::save_outputs(&path, std::slice::from_ref(&head))
                .unwrap()
                .is_none()
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["terminal"], initial["terminal"]);
        assert_eq!(saved["keyboard"], initial["keyboard"]);
        assert_eq!(saved["outputs"]["DP-9"], initial["outputs"]["DP-9"]);
        assert!(Commands::load(&path).unwrap().outputs["DP-1"].adaptive_sync);
        assert_eq!(Commands::load(&path).unwrap().outputs["DP-1"].rotation, head.rotation);
        assert_eq!(saved["outputs"]["DP-1"]["color"], "hdr10");
        assert_eq!(Commands::load(&path).unwrap().outputs["DP-1"].color, head.color);
        assert!(saved["outputs"]["DP-1"].get("hdr10").is_none());
        assert!(saved["outputs"]["DP-1"].get("revision").is_none());
        assert!(saved["outputs"]["DP-1"].get("effective").is_none());
        assert!(saved["outputs"]["DP-1"].get("pending").is_none());
        assert_eq!(Commands::load(&path).unwrap().outputs["DP-1"].scale, 1.5);
        assert_eq!(
            Commands::load(&path).unwrap().outputs["DP-1"].refresh_mhz,
            Some(144000)
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        let mut disabled = head;
        disabled.current = None;
        assert!(
            Commands::save_outputs(&path, &[disabled])
                .unwrap()
                .is_none()
        );
        let output = &Commands::load(&path).unwrap().outputs["DP-1"];
        assert!(!output.enabled);
        assert_eq!(output.mode, Some([2560, 1440]));
        assert_eq!(output.scale, 1.5);
    }

    #[test]
    fn display_save_preserves_invalid_or_unwritable_configuration_for_retry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        for bytes in [
            b"{broken".as_slice(),
            b"{\"keyboard\":{\"repeat_rate\":999}}".as_slice(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(Commands::save_outputs(&path, &[]).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        }
        assert!(Commands::save_outputs(&directory.path().join("missing/park.json"), &[]).is_err());
        assert!(!directory.path().join("missing").exists());
        std::fs::remove_file(&path).unwrap();
        assert!(Commands::save_outputs(&path, &[]).unwrap().is_none());
        assert!(Commands::load(&path).unwrap().outputs.is_empty());
    }

    #[test]
    fn directory_sync_failure_after_replacement_keeps_the_confirmed_display_and_reports_a_warning()
    {
        use lince_interface::host::display::{DisplayAction, DisplayOutcome, DisplayRequest};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        std::fs::write(&path, br#"{"keyboard":{"repeat_rate":35}}"#).unwrap();
        let mut server = crate::server::Server::new([800, 600]).unwrap();
        server.registered_output_catalog().unwrap();
        let snapshot = server.display_snapshot();
        let mut heads = snapshot.configuration();
        heads.values_mut().next().unwrap().as_mut().unwrap().scale = 1.25;
        server.display_request(
            DisplayRequest {
                id: 1,
                action: DisplayAction::Preview {
                    revision: snapshot.revision,
                    heads,
                },
            },
            |_| panic!("A preview must not save output preferences"),
        );
        for _ in 0..2 {
            let requests = server.output_requests();
            assert_eq!(requests.len(), 1);
            let request = &requests[0];
            assert!(
                server
                    .apply_virtual_output_configuration(request, [800, 600])
                    .unwrap()
            );
            server.output_configuration_finished(request.token, true);
        }
        let token = server.display_preview_token().unwrap();
        let mut warning = None;
        server.display_request(
            DisplayRequest {
                id: 2,
                action: DisplayAction::Keep { token },
            },
            |heads| {
                warning = Commands::save_outputs_with_sync(&path, heads, |_| {
                    Err(io::Error::other("test directory sync failure"))
                })?;
                Ok(())
            },
        );
        assert!(
            warning
                .as_deref()
                .is_some_and(|warning| warning.contains("were saved")
                    && warning.contains("test directory sync failure"))
        );
        assert_eq!(
            server.display_snapshot().response.unwrap().outcome,
            DisplayOutcome::Kept
        );
        assert_eq!(server.kept_display_token(), Some(token));
        assert!(server.display_preview_token().is_none());
        assert!(server.output_requests().is_empty());
        let saved = Commands::load(&path).unwrap();
        assert_eq!(saved.outputs.values().next().unwrap().scale, 1.25);
        assert_eq!(saved.keyboard.repeat_rate, 35);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn output_preferences_are_bounded_and_do_not_accept_unknown_keys() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        std::fs::write(&path, r#"{"outputs":{"DP-1":{"scale":1.25,"position":[0,0],"mode":[2560,1440],"refresh_mhz":60000}}}"#).unwrap();
        assert_eq!(Commands::load(&path).unwrap().outputs["DP-1"].scale, 1.25);
        for configuration in [
            r#"{"outputs":{"DP-1":{"scale":0}}}"#,
            r#"{"outputs":{"DP-1":{"scale":1.234}}}"#,
            r#"{"outputs":{"DP-1":{"position":[-10,0]}}}"#,
            r#"{"outputs":{"DP-1":{"mode":[0,1440]}}}"#,
            r#"{"outputs":{"DP-1":{"refesh_mhz":60000}}}"#,
        ] {
            std::fs::write(&path, configuration).unwrap();
            assert!(Commands::load(&path).is_err());
        }
    }

    #[test]
    fn commands_are_explicit_local_argv_and_reject_invalid_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        assert!(
            Commands::load(&path)
                .unwrap()
                .command(DesktopAction::OpenLauncher)
                .is_err()
        );
        std::fs::write(&path, r#"{"launcher":{"program":"chosen-launcher","arguments":["$(touch /tmp/unexpected)","two words"],"directory":null}}"#).unwrap();
        let command = Commands::load(&path)
            .unwrap()
            .command(DesktopAction::OpenLauncher)
            .unwrap();
        assert_eq!(command.arguments, ["$(touch /tmp/unexpected)", "two words"]);
        std::fs::write(
            &path,
            r#"{"launcher":{"program":"","arguments":[],"directory":null}}"#,
        )
        .unwrap();
        assert!(Commands::load(&path).is_err());
        std::fs::write(&path, r#"{"autostart":[]}"#).unwrap();
        assert!(Commands::load(&path).is_err());
        std::fs::write(
            &path,
            r#"{"terminal":{"program":"chosen-terminal","argument":["ignored"]}}"#,
        )
        .unwrap();
        assert!(Commands::load(&path).is_err());
    }
}
