use crate::{Result, serial::Cancellation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha384};
use std::{
    collections::VecDeque,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

const SDK: &str = "5.5.2";
const NIX_SDK: &str =
    "github:mirrexagon/nixpkgs-esp-dev/5287d6e1ca9e15ebd5113c41b9590c468e1e001b#esp32s3-idf";

pub fn source_files() -> Vec<(&'static str, &'static str)> {
    macro_rules! source {
        ($path:literal) => {
            ($path, include_str!(concat!("../firmware/esp-idf/", $path)))
        };
    }
    vec![
        ("LICENSE", include_str!("../../../LICENSE")),
        ("TESTING.txt", include_str!("../TESTING.txt")),
        source!("CREDITS.txt"),
        source!("licenses/esp-idf-Apache-2.0.txt"),
        source!("licenses/esp32-camera-Apache-2.0.txt"),
        source!("licenses/esp-websocket-client-Apache-2.0.txt"),
        source!("licenses/esp-jpeg-Apache-2.0.txt"),
        source!("licenses/TJpgDec.txt"),
        source!("CMakeLists.txt"),
        source!("cmake/board.cmake"),
        source!(".gitignore"),
        source!("sdkconfig.defaults"),
        source!("dependencies.lock"),
        source!("resources.json"),
        source!("fixtures/decimals.json"),
        source!("main/CMakeLists.txt"),
        source!("main/idf_component.yml"),
        source!("main/main.c"),
        source!("components/lince_core/CMakeLists.txt"),
        source!("components/lince_core/include/lince_core.h"),
        source!("components/lince_core/configuration.c"),
        source!("components/lince_core/identity.c"),
        source!("components/lince_core/network.c"),
        source!("components/lince_core/protocol.c"),
        source!("components/lince_core/outbox.c"),
        source!("components/lince_core/stream.c"),
        source!("components/lince_core/lifecycle.c"),
        source!("components/lince_core/include/lince_stream.h"),
        source!("components/lince_core/include/lince_provision.h"),
        source!("components/lince_core/include/lince_outbox.h"),
        source!("components/lince_core/provisioning.c"),
        source!("components/lince_core/test/CMakeLists.txt"),
        source!("components/lince_core/test/protocol_test.c"),
        source!("components/lince_core/test/outbox_test.c"),
        source!("components/lince_core/test/lifecycle_test.c"),
        source!("components/lince_workflows/CMakeLists.txt"),
        source!("components/lince_workflows/Kconfig"),
        source!("components/lince_workflows/include/lince_workflows.h"),
        source!("components/lince_workflows/camera.c"),
        source!("components/lince_workflows/scalar.c"),
        source!("components/lince_workflows/custom.c"),
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub version: u32,
    pub name: String,
    pub chip: String,
    pub idf: String,
    pub workflows: Vec<String>,
    pub license: String,
}

impl Default for Package {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Lince ESP camera and scalar starters".into(),
            chip: "esp32s3".into(),
            idf: SDK.into(),
            workflows: vec![
                "camera".into(),
                "scalar".into(),
                "custom".into(),
                "scalar-demo".into(),
            ],
            license: "MIT".into(),
        }
    }
}

impl Package {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1
            || self.chip != "esp32s3"
            || self.idf != SDK
            || self.name.trim().is_empty()
            || self.name.len() > 128
            || self.workflows.is_empty()
            || self.workflows.len() > 16
            || self.workflows.iter().any(|name| {
                name.is_empty()
                    || name.len() > 64
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            })
            || self.license.is_empty()
            || self.license.len() > 128
        {
            return Err("Invalid firmware package or unsupported target/SDK".into());
        }
        Ok(())
    }
}

pub fn export(destination: &Path) -> Result<()> {
    export_for_board(destination, crate::profile::DEFAULT_BOARD)
}

pub fn export_for_board(destination: &Path, board_id: &str) -> Result<()> {
    let profile = crate::profile::builtin(board_id)?;
    if destination.exists()
        && fs::read_dir(destination)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("Choose an empty folder for the editable firmware project".into());
    }
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    if fs::symlink_metadata(destination)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Firmware export folder cannot be a symlink".into());
    }
    for (name, text) in source_files() {
        let path = destination.join(name);
        fs::create_dir_all(path.parent().ok_or("Invalid firmware path")?)
            .map_err(|e| e.to_string())?;
        use std::io::Write;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|e| e.to_string())?;
    }
    for (id, text) in crate::profile::catalog() {
        let directory = destination.join("boards");
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        fs::write(directory.join(format!("{id}.json")), text).map_err(|e| e.to_string())?;
    }
    fs::write(
        destination.join("board.json"),
        serde_json::to_vec_pretty(&profile).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        destination.join("lince-firmware.json"),
        serde_json::to_vec_pretty(&Package::default()).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn select_board(root: &Path, board_id: &str) -> Result<crate::profile::BoardProfile> {
    let profile = crate::profile::builtin(board_id)?;
    let package: Package =
        serde_json::from_slice(&regular_bytes(&root.join("lince-firmware.json"), 16384)?)
            .map_err(|_| "Invalid firmware package manifest")?;
    package.validate()?;
    regular_bytes(&root.join("board.json"), 8192)?;
    fs::write(
        root.join("board.json"),
        serde_json::to_vec_pretty(&profile).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(profile)
}

fn digest(bytes: &[u8]) -> String {
    Sha384::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn regular_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit {
        return Err("Firmware input must be a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Firmware input grew beyond its budget".into());
    }
    Ok(bytes)
}

fn source_digest(root: &Path) -> Result<String> {
    fn visit(
        root: &Path,
        directory: &Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
        entries: &mut usize,
    ) -> Result<()> {
        if depth > 32 {
            return Err("Firmware source folders are nested too deeply".into());
        }
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let name = entry.file_name();
            if (directory == root && name == "build") || name == ".git" || name == "sdkconfig.old" {
                continue;
            }
            *entries += 1;
            if files.len() > 4096 || *entries > 8192 {
                return Err("Firmware project has too many source files".into());
            }
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("Firmware source symlinks are not supported".into());
            }
            if kind.is_dir() {
                visit(root, &path, files, depth + 1, entries)?;
            } else if kind.is_file() {
                files.push(
                    path.strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_path_buf(),
                );
            } else {
                return Err("Firmware project contains a non-file input".into());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files, 0, &mut 0)?;
    files.sort();
    let mut hash = Sha384::new();
    let mut total = 0_usize;
    for path in files {
        let bytes = regular_bytes(&root.join(&path), 8 * 1024 * 1024)?;
        total = total.saturating_add(bytes.len());
        if total > 32 * 1024 * 1024 {
            return Err("Firmware sources exceed 32 MiB".into());
        }
        let name = path.to_str().ok_or("Firmware source path must be UTF-8")?;
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Image {
    pub offset: u32,
    pub path: String,
    pub sha384: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub version: u32,
    pub mode: FirmwareMode,
    pub chip: String,
    pub idf: String,
    pub source_sha384: String,
    pub images: Vec<Image>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FirmwareMode {
    Workflow,
    Tests,
}

impl Artifact {
    pub fn fingerprint(&self) -> Result<String> {
        Ok(digest(
            &serde_json::to_vec(self).map_err(|e| e.to_string())?,
        ))
    }

    pub fn verify(&self, root: &Path) -> Result<Vec<Vec<u8>>> {
        board(root)?;
        if self.version != 1
            || self.chip != "esp32s3"
            || self.idf != SDK
            || self.images.len() != 3
            || source_digest(root)? != self.source_sha384
        {
            return Err("Firmware source or target changed; compile the project again".into());
        }
        let build = root
            .join("build")
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let mut ranges = std::collections::BTreeSet::new();
        let mut outputs = Vec::new();
        for image in &self.images {
            let relative = Path::new(&image.path);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
            {
                return Err("Invalid artifact path".into());
            }
            let path = build.join(relative);
            if !path
                .canonicalize()
                .map_err(|e| e.to_string())?
                .starts_with(&build)
            {
                return Err("Artifact escaped its build folder".into());
            }
            let bytes = regular_bytes(&path, 4 * 1024 * 1024)?;
            let maximum = match image.offset {
                0 => 0x8000,
                0x8000 => 0x1000,
                0x10000 => 0x3f0000,
                _ => {
                    return Err(
                        "Artifact would overwrite protected configuration/identity storage".into(),
                    );
                }
            };
            if bytes.is_empty()
                || bytes.len() > maximum
                || !ranges.insert(image.offset)
                || digest(&bytes) != image.sha384
            {
                return Err("Firmware image changed or has an invalid flash range".into());
            }
            outputs.push(bytes);
        }
        let table = self
            .images
            .iter()
            .position(|image| image.offset == 0x8000)
            .ok_or("Firmware has no partition table")?;
        let app = self
            .images
            .iter()
            .position(|image| image.offset == 0x10000)
            .ok_or("Firmware has no application")?;
        validate_partitions(&outputs[table], outputs[app].len())?;
        Ok(outputs)
    }
}

fn validate_partitions(table: &[u8], app_bytes: usize) -> Result<()> {
    let mut nvs = false;
    let mut app = false;
    for entry in table.chunks_exact(32) {
        if entry[..2] != 0x50aa_u16.to_le_bytes() {
            break;
        }
        let offset = u32::from_le_bytes(
            entry[4..8]
                .try_into()
                .map_err(|_| "Invalid partition offset")?,
        );
        let size = u32::from_le_bytes(
            entry[8..12]
                .try_into()
                .map_err(|_| "Invalid partition size")?,
        );
        let flags = u32::from_le_bytes(
            entry[28..32]
                .try_into()
                .map_err(|_| "Invalid partition flags")?,
        );
        if flags != 0 {
            return Err("Protected/encrypted partition changes require a separately reviewed installation procedure".into());
        }
        match (entry[2], entry[3]) {
            (1, 2) if offset == 0x9000 && size == 0x6000 && !nvs => nvs = true,
            (1, 1) if offset == 0xf000 && size == 0x1000 => {}
            (0, 0)
                if offset == 0x10000 && size <= 0x3f0000 && app_bytes <= size as usize && !app =>
            {
                app = true
            }
            _ => {
                return Err(
                    "Partition layout would change the retained identity/configuration storage"
                        .into(),
                );
            }
        }
    }
    if !nvs || !app {
        return Err(
            "Firmware must retain its NVS partition and a bounded factory application".into(),
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Toolchain {
    #[default]
    Nix,
    Environment,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChipProbe {
    pub chip: String,
    pub mac: String,
    pub flash_bytes: u32,
}

fn parse_probe(output: &str) -> Result<ChipProbe> {
    if !output
        .lines()
        .any(|line| line.trim().starts_with("Chip is ESP32-S3"))
    {
        return Err("The selected port did not identify an ESP32-S3".into());
    }
    let mac = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("MAC:"))
        .ok_or("Chip did not report its hardware identity")?
        .trim()
        .to_lowercase();
    if mac.len() != 17
        || !mac
            .split(':')
            .all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Invalid chip hardware identity".into());
    }
    let size = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("Detected flash size:"))
        .ok_or("Chip did not report its flash capacity")?
        .trim();
    let flash_bytes = if let Some(mb) = size.strip_suffix("MB") {
        mb.parse::<u32>()
            .ok()
            .and_then(|n| n.checked_mul(1024 * 1024))
    } else if let Some(kb) = size.strip_suffix("KB") {
        kb.parse::<u32>().ok().and_then(|n| n.checked_mul(1024))
    } else {
        None
    }
    .ok_or("Invalid flash capacity")?;
    Ok(ChipProbe {
        chip: "esp32s3".into(),
        mac,
        flash_bytes,
    })
}

fn probe_unlocked(
    port: &str,
    toolchain: Toolchain,
    cancellation: &Cancellation,
) -> Result<ChipProbe> {
    if port.is_empty() || port.starts_with('-') || port.contains('\0') {
        return Err("Select a serial port".into());
    }
    let mut command = toolchain.command("python");
    command.args([
        "-m", "esptool", "--chip", "esp32s3", "--port", port, "flash_id",
    ]);
    parse_probe(&run(command, cancellation)?)
}

pub fn probe(port: &str, toolchain: Toolchain, cancellation: &Cancellation) -> Result<ChipProbe> {
    let _lease = crate::serial::lease(port)?;
    probe_unlocked(port, toolchain, cancellation)
}

impl Toolchain {
    fn command(self, tool: &str) -> Command {
        match self {
            Self::Nix => {
                let mut command = Command::new("nix");
                command
                    .env_remove("LD_LIBRARY_PATH")
                    .env_remove("LD_PRELOAD");
                command.args(["develop", NIX_SDK, "--command", tool]);
                command
            }
            Self::Environment => Command::new(tool),
        }
    }
}

fn tail(mut input: impl Read) -> String {
    let mut bytes = VecDeque::with_capacity(65536);
    let mut buffer = [0; 4096];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => {
                for byte in &buffer[..length] {
                    if bytes.len() == 65536 {
                        bytes.pop_front();
                    }
                    bytes.push_back(*byte);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&bytes.into_iter().collect::<Vec<_>>()).into_owned()
}

fn stop_descendants(id: u32) {
    #[cfg(unix)]
    if let Some(pid) = rustix::process::Pid::from_raw(id as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    #[cfg(not(unix))]
    {
        let _ = id;
    }
}

fn run(mut command: Command, cancellation: &Cancellation) -> Result<String> {
    cancellation.check()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start firmware tool: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Firmware tool output unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Firmware tool diagnostics unavailable")?;
    let output = std::thread::spawn(move || tail(stdout));
    let errors = std::thread::spawn(move || tail(stderr));
    let status = loop {
        if cancellation.check().is_err() {
            stop_descendants(child.id());
            let _ = child.kill();
            let _ = child.wait();
            let _ = output.join();
            let _ = errors.join();
            return Err(
                "Firmware operation cancelled; installation may require cable recovery".into(),
            );
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                stop_descendants(child.id());
                let _ = child.kill();
                let _ = child.wait();
                let _ = output.join();
                let _ = errors.join();
                return Err(format!("Firmware tool status unavailable: {error}"));
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    stop_descendants(child.id());
    let stdout = output.join().map_err(|_| "Firmware output worker failed")?;
    let stderr = errors
        .join()
        .map_err(|_| "Firmware diagnostic worker failed")?;
    if !status.success() {
        return Err(format!(
            "Firmware tool failed ({status}):\n{stderr}\n{stdout}"
        ));
    }
    Ok(stdout)
}

pub fn compile(root: &Path, toolchain: Toolchain, cancellation: &Cancellation) -> Result<Artifact> {
    compile_mode(root, toolchain, cancellation, FirmwareMode::Workflow)
}

pub fn compile_tests(
    root: &Path,
    toolchain: Toolchain,
    cancellation: &Cancellation,
) -> Result<Artifact> {
    compile_mode(root, toolchain, cancellation, FirmwareMode::Tests)
}

fn compile_mode(
    root: &Path,
    toolchain: Toolchain,
    cancellation: &Cancellation,
    mode: FirmwareMode,
) -> Result<Artifact> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let package: Package =
        serde_json::from_slice(&regular_bytes(&root.join("lince-firmware.json"), 16384)?)
            .map_err(|_| "Invalid firmware package manifest")?;
    package.validate()?;
    board(&root)?.validate()?;
    source_digest(&root)?;
    let mut version = toolchain.command("idf.py");
    version.arg("--version").current_dir(&root);
    if !run(version, cancellation)?
        .trim()
        .ends_with(&format!("v{SDK}"))
    {
        return Err("Firmware requires the pinned ESP-IDF 5.5.2 toolchain".into());
    }
    let mut configure = toolchain.command("idf.py");
    configure
        .args(["-B", "build", "-DIDF_TARGET=esp32s3"])
        .arg(if mode == FirmwareMode::Tests {
            "-DLINCE_RUN_TESTS=ON"
        } else {
            "-DLINCE_RUN_TESTS=OFF"
        })
        .arg(if mode == FirmwareMode::Tests {
            "-DLINCE_COMPILE_TESTS=ON"
        } else {
            "-DLINCE_COMPILE_TESTS=OFF"
        })
        .arg("reconfigure")
        .current_dir(&root);
    run(configure, cancellation)?;
    let before = source_digest(&root)?;
    let mut command = toolchain.command("idf.py");
    command
        .args(["-B", "build", "-DIDF_TARGET=esp32s3", "build"])
        .current_dir(&root);
    run(command, cancellation)?;
    if before != source_digest(&root)? {
        return Err("Firmware sources changed during compilation; compile again".into());
    }
    let build = root.join("build");
    let arguments: serde_json::Value =
        serde_json::from_slice(&regular_bytes(&build.join("flasher_args.json"), 65536)?)
            .map_err(|_| "Invalid SDK flash manifest")?;
    let files = arguments["flash_files"]
        .as_object()
        .ok_or("SDK did not produce flash images")?;
    let mut images = Vec::new();
    for (offset, path) in files {
        let offset = u32::from_str_radix(offset.trim_start_matches("0x"), 16)
            .map_err(|_| "Invalid flash offset")?;
        let path = path
            .as_str()
            .ok_or("Invalid firmware image path")?
            .to_string();
        let canonical = build
            .join(&path)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !canonical.starts_with(build.canonicalize().map_err(|e| e.to_string())?) {
            return Err("SDK image escaped build folder".into());
        }
        images.push(Image {
            offset,
            path,
            sha384: digest(&regular_bytes(&canonical, 4 * 1024 * 1024)?),
        });
    }
    let artifact = Artifact {
        version: 1,
        mode,
        chip: package.chip,
        idf: package.idf,
        source_sha384: source_digest(&root)?,
        images,
    };
    artifact.verify(&root)?;
    fs::write(
        build.join("lince-artifact.json"),
        serde_json::to_vec_pretty(&artifact).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(artifact)
}

pub fn artifact(root: &Path) -> Result<Artifact> {
    serde_json::from_slice(&regular_bytes(
        &root.join("build/lince-artifact.json"),
        65536,
    )?)
    .map_err(|_| "Invalid firmware artifact manifest".into())
}

pub fn board(root: &Path) -> Result<crate::profile::BoardProfile> {
    let profile: crate::profile::BoardProfile =
        serde_json::from_slice(&regular_bytes(&root.join("board.json"), 8192)?)
            .map_err(|_| "Invalid board profile")?;
    profile.validate()?;
    Ok(profile)
}

pub fn flash(
    root: &Path,
    artifact: &Artifact,
    expected_fingerprint: &str,
    port: &str,
    expected_mac: &str,
    toolchain: Toolchain,
    cancellation: &Cancellation,
) -> Result<()> {
    if port.is_empty() || port.starts_with('-') || port.contains('\0') {
        return Err("Select a serial port".into());
    }
    if artifact.fingerprint()? != expected_fingerprint {
        return Err("Firmware selection changed; review the artifact again".into());
    }
    let images = artifact.verify(root)?;
    let profile = board(root)?;
    let _lease = crate::serial::lease(port)?;
    let chip = probe_unlocked(port, toolchain, cancellation)?;
    validate_hardware(&chip, &profile, expected_mac)?;
    let staging = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut command = toolchain.command("python");
    command.args([
        "-m",
        "esptool",
        "--chip",
        "esp32s3",
        "--port",
        port,
        "--baud",
        "460800",
        "write_flash",
    ]);
    for (index, (image, bytes)) in artifact.images.iter().zip(images).enumerate() {
        let path = staging.path().join(format!("image-{index}.bin"));
        fs::write(&path, bytes).map_err(|e| e.to_string())?;
        command.arg(format!("0x{:x}", image.offset)).arg(path);
    }
    run(command, cancellation)?;
    Ok(())
}

fn validate_hardware(
    chip: &ChipProbe,
    profile: &crate::profile::BoardProfile,
    expected_mac: &str,
) -> Result<()> {
    if chip.mac != expected_mac
        || chip.chip != profile.chip
        || chip.flash_bytes < profile.minimum_flash_bytes
    {
        return Err("Selected hardware differs from the confirmed MAC, chip or board profile's minimum flash capacity".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn completed_tools_cannot_leave_descendants_holding_output_pipes_open() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & printf ready"]);
        let start = std::time::Instant::now();
        assert_eq!(run(command, &Cancellation::default()).unwrap(), "ready");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    fn partition_table() -> Vec<u8> {
        let mut table = Vec::new();
        for (kind, subtype, offset, size) in [
            (1, 2, 0x9000_u32, 0x6000_u32),
            (1, 1, 0xf000, 0x1000),
            (0, 0, 0x10000, 0x177000),
        ] {
            let mut entry = [0_u8; 32];
            entry[..2].copy_from_slice(&0x50aa_u16.to_le_bytes());
            entry[2] = kind;
            entry[3] = subtype;
            entry[4..8].copy_from_slice(&offset.to_le_bytes());
            entry[8..12].copy_from_slice(&size.to_le_bytes());
            table.extend_from_slice(&entry);
        }
        table
    }

    #[test]
    fn hardware_probe_binds_target_mac_and_capacity_before_any_installation() {
        let chip = parse_probe("Chip is ESP32-S3 (QFN56) (revision v0.2)\nMAC: 12:34:56:78:9A:BC\nDetected flash size: 16MB\n").unwrap();
        assert_eq!(chip.mac, "12:34:56:78:9a:bc");
        assert_eq!(chip.flash_bytes, 16 * 1024 * 1024);
        assert!(
            parse_probe("Chip is ESP32\nMAC: 12:34:56:78:9a:bc\nDetected flash size: 16MB")
                .is_err()
        );
        assert!(
            parse_probe("Chip is ESP32-S3\nMAC: malformed\nDetected flash size: 16MB").is_err()
        );
        let mut table = partition_table();
        validate_partitions(&table, 1024).unwrap();
        table[4..8].copy_from_slice(&0x80000_u32.to_le_bytes());
        assert!(validate_partitions(&table, 1024).is_err());
    }

    #[test]
    fn tool_output_is_bounded_and_cancelled_jobs_never_spawn() {
        let output = tail(std::io::Cursor::new(vec![b'x'; 100_000]));
        assert_eq!(output.len(), 65536);
        let cancellation = Cancellation::default();
        cancellation.cancel();
        let error = run(Command::new("this-program-must-never-start"), &cancellation).unwrap_err();
        assert_eq!(error, "Operation cancelled");
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_terminates_an_active_tool_and_its_children() {
        let directory = tempfile::tempdir().unwrap();
        let ready = directory.path().join("ready");
        let cancellation = Cancellation::default();
        let token = cancellation.clone();
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & touch \"$1\"; wait", "ailuros-test"]);
        command.arg(&ready);
        let worker = std::thread::spawn(move || run(command, &token));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !ready.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let started = ready.exists();
        cancellation.cancel();
        let error = worker.join().unwrap().unwrap_err();
        assert!(started, "tool did not reach its active state");
        assert!(error.contains("cancelled"));
        assert!(std::time::Instant::now() < deadline);
    }

    #[test]
    fn export_is_editable_and_never_overwrites_existing_work() {
        let folder = tempfile::tempdir().unwrap();
        export(folder.path()).unwrap();
        let package: Package =
            serde_json::from_slice(&fs::read(folder.path().join("lince-firmware.json")).unwrap())
                .unwrap();
        package.validate().unwrap();
        let before = source_digest(folder.path()).unwrap();
        fs::write(folder.path().join("main/main.c"), "custom workflow").unwrap();
        assert_ne!(before, source_digest(folder.path()).unwrap());
        assert!(export(folder.path()).is_err());
        assert_eq!(
            fs::read_to_string(folder.path().join("main/main.c")).unwrap(),
            "custom workflow"
        );
    }

    #[test]
    fn exports_and_profile_switches_preserve_custom_source_and_have_no_private_state() {
        for (id, _) in crate::profile::catalog() {
            let folder = tempfile::tempdir().unwrap();
            export_for_board(folder.path(), id).unwrap();
            let selected = board(folder.path()).unwrap();
            assert_eq!(selected.name, crate::profile::builtin(id).unwrap().name);
            assert!(folder.path().join("cmake/board.cmake").is_file());
            assert!(folder.path().join("boards/esp32s3.json").is_file());
            for absent in [
                "sdkconfig",
                "build",
                "managed_components",
                "keys",
                "tls",
                "enrollment.json",
            ] {
                assert!(!folder.path().join(absent).exists());
            }
            let custom = folder.path().join("components/lince_workflows/custom.c");
            fs::write(&custom, "owner's workflow").unwrap();
            let before = source_digest(folder.path()).unwrap();
            let other = if id == crate::profile::DEFAULT_BOARD {
                "goouuu-esp32s3-cam-v1.5"
            } else {
                crate::profile::DEFAULT_BOARD
            };
            select_board(folder.path(), other).unwrap();
            assert_ne!(source_digest(folder.path()).unwrap(), before);
            assert_eq!(fs::read_to_string(custom).unwrap(), "owner's workflow");
        }
        let folder = tempfile::tempdir().unwrap();
        assert!(export_for_board(folder.path(), "../unknown").is_err());
        assert!(folder.path().read_dir().unwrap().next().is_none());
    }

    #[test]
    fn board_minimum_capacity_is_checked_independently_of_the_safe_image_layout() {
        let mut chip = ChipProbe {
            chip: "esp32s3".into(),
            mac: "12:34:56:78:9a:bc".into(),
            flash_bytes: 4 * 1024 * 1024,
        };
        let generic = crate::profile::builtin(crate::profile::DEFAULT_BOARD).unwrap();
        validate_hardware(&chip, &generic, &chip.mac).unwrap();
        let camera = crate::profile::builtin("goouuu-esp32s3-cam-v1.5").unwrap();
        assert!(validate_hardware(&chip, &camera, &chip.mac).is_err());
        chip.flash_bytes = 16 * 1024 * 1024;
        validate_hardware(&chip, &camera, &chip.mac).unwrap();
        assert!(validate_hardware(&chip, &camera, "another-device").is_err());
        chip.chip = "esp32".into();
        assert!(validate_hardware(&chip, &camera, &chip.mac).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn selecting_a_profile_refuses_symlinks_without_touching_their_target() {
        let folder = tempfile::tempdir().unwrap();
        export(folder.path()).unwrap();
        let original = folder.path().join("original-board.json");
        fs::rename(folder.path().join("board.json"), &original).unwrap();
        let before = fs::read(&original).unwrap();
        std::os::unix::fs::symlink(&original, folder.path().join("board.json")).unwrap();
        assert!(select_board(folder.path(), "goouuu-esp32s3-cam-v1.5").is_err());
        assert_eq!(fs::read(original).unwrap(), before);
    }

    #[test]
    fn artifact_verification_rejects_changes_and_configuration_writes() {
        let folder = tempfile::tempdir().unwrap();
        export(folder.path()).unwrap();
        let build = folder.path().join("build");
        fs::create_dir(&build).unwrap();
        let mut images = Vec::new();
        for (index, offset) in [0, 0x8000, 0x10000].into_iter().enumerate() {
            let name = format!("image-{index}.bin");
            let bytes = if offset == 0x8000 {
                partition_table()
            } else {
                vec![index as u8; 64]
            };
            fs::write(build.join(&name), &bytes).unwrap();
            images.push(Image {
                offset,
                path: name,
                sha384: digest(&bytes),
            });
        }
        let mut artifact = Artifact {
            version: 1,
            mode: FirmwareMode::Workflow,
            chip: "esp32s3".into(),
            idf: SDK.into(),
            source_sha384: source_digest(folder.path()).unwrap(),
            images,
        };
        assert_eq!(artifact.verify(folder.path()).unwrap().len(), 3);
        let original_fingerprint = artifact.fingerprint().unwrap();
        artifact.mode = FirmwareMode::Tests;
        assert_ne!(artifact.fingerprint().unwrap(), original_fingerprint);
        artifact.mode = FirmwareMode::Workflow;
        artifact.images[0].offset = 0x9000;
        assert!(artifact.verify(folder.path()).is_err());
        artifact.images[0].offset = 0;
        artifact.images[0].path = "../outside.bin".into();
        assert!(artifact.verify(folder.path()).is_err());
        artifact.images[0].path = "image-0.bin".into();
        fs::write(build.join("image-0.bin"), [9; 64]).unwrap();
        assert!(artifact.verify(folder.path()).is_err());
        fs::write(build.join("image-0.bin"), [0; 64]).unwrap();
        select_board(folder.path(), "goouuu-esp32s3-cam-v1.5").unwrap();
        assert!(artifact.verify(folder.path()).is_err());
    }
}
