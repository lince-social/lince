extern crate self as lince_interface;

pub mod composition;
pub mod configuration;
pub mod dependency_graph;
pub mod domain_model;
pub mod frame;
#[cfg(feature = "browser-parity")]
pub mod html;
pub mod input;
pub mod official_runtime;
pub mod official_sands;
pub mod primitive_gallery;
pub mod retained_ui;
pub mod sand;
pub mod semantic;
pub mod style;
pub mod update_panel;
pub mod window_stress;
pub mod workspace;

#[cfg(feature = "physics-preflight")]
pub mod spatial;

#[cfg(feature = "semantic-spatial")]
pub mod scene_artifact;

#[cfg(feature = "native-runtime")]
pub mod node_layer;

#[cfg(feature = "native-runtime")]
pub mod domain;

#[cfg(feature = "native-runtime")]
pub mod joined_accessibility;
#[cfg(feature = "native-runtime")]
pub mod joined_panel;

#[cfg(feature = "bevy-preflight")]
pub mod bevy_host;

#[cfg(feature = "bevy-preflight")]
pub mod bevy_layer;

use crate::input::{InputEnvelope, InputEvidence};
use crate::window_stress::{WindowStress, WindowStressFacts};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const REPORT_SCHEMA_VERSION: u32 = 7;

#[derive(Clone, Debug, Serialize)]
pub struct DependencyFact {
    pub name: String,
    pub revision: String,
    pub role: String,
    pub state: String,
    pub license: String,
    pub gpu_family: Option<String>,
    pub raw_window_handle_family: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdapterFact {
    pub name: String,
    pub state: String,
    pub owner: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HostFacts {
    pub operating_system: String,
    pub architecture: String,
    pub event_loop_owner: String,
    pub rendering_device_owner: String,
    pub final_compositor_owner: String,
    pub gpu_name: String,
    pub gpu_backend: String,
    pub gpu_device_type: String,
    pub gpu_driver: String,
    pub gpu_driver_info: String,
    pub surface_format: String,
    pub present_mode: String,
    pub scale_factor: f64,
    pub surface_width: u32,
    pub surface_height: u32,
    pub initialization_error: Option<String>,
}

impl HostFacts {
    pub fn unavailable(scale_factor: f64, width: u32, height: u32, error: String) -> Self {
        Self {
            operating_system: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            event_loop_owner: "lince-winit-host".into(),
            rendering_device_owner: "unavailable".into(),
            final_compositor_owner: "unavailable".into(),
            gpu_name: "unavailable".into(),
            gpu_backend: "unavailable".into(),
            gpu_device_type: "unavailable".into(),
            gpu_driver: "unavailable".into(),
            gpu_driver_info: "unavailable".into(),
            surface_format: "unavailable".into(),
            present_mode: "unavailable".into(),
            scale_factor,
            surface_width: width,
            surface_height: height,
            initialization_error: Some(error),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LaboratoryReport {
    pub schema_version: u32,
    pub gate: String,
    pub status: String,
    pub created_unix_millis: u128,
    pub git_revision: String,
    pub host: HostFacts,
    pub runtime: RuntimeFacts,
    pub dependencies: Vec<DependencyFact>,
    pub adapters: Vec<AdapterFact>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RuntimeFacts {
    pub host_redraw_events: u64,
    pub host_frames_presented: u64,
    pub host_queue_submissions: u64,
    pub host_command_buffers_submitted: u64,
    pub last_submission_participants: Vec<String>,
    pub bevy_updates: u64,
    pub bevy_output_command_buffers_handed_to_host: u64,
    pub bevy_direct_queue_submissions: u64,
    pub keyboard_input_events: u64,
    pub pointer_input_events: u64,
    pub focus_events: u64,
    pub ime_events: u64,
    pub normalized_input_events: u64,
    pub rejected_input_events: u64,
    pub last_normalized_input: Option<InputEnvelope>,
    pub last_input_target_local: Option<[f64; 2]>,
    pub last_input_rejection: Option<String>,
    pub window_stress: WindowStressFacts,
    pub teardown_started: bool,
    pub teardown_complete: bool,
    pub teardown_failures: u64,
    pub gpu_queue_idle_before_exit: Option<bool>,
    pub external_slot_teardown: String,
    pub graceful_exit_requested: bool,
    pub input_to_submit_samples: u64,
    pub last_input_to_submit_micros: u128,
    pub maximum_input_to_submit_micros: u128,
    pub resize_events: u64,
    pub scale_factor_events: u64,
    pub surface_recoveries: u64,
    pub surface_recovery_failures: u64,
    pub last_presented_frame_cpu_micros: u128,
    pub maximum_presented_frame_cpu_micros: u128,
}

impl LaboratoryReport {
    pub fn ownership_preflight(host: HostFacts) -> Self {
        let status = if host.initialization_error.is_some() {
            "unavailable"
        } else {
            "running-incomplete"
        };

        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            gate: "native render and input seam".into(),
            status: status.into(),
            created_unix_millis: unix_millis(),
            git_revision: git_revision(),
            host,
            runtime: RuntimeFacts::default(),
            dependencies: dependency_facts(),
            adapters: adapter_facts(),
            limitations: vec![
                "Bevy manual rendering is not integrated yet.".into(),
                "The host currently proves only a human-runnable Lince-owned winit/wgpu panel and report surface.".into(),
            ],
        }
    }

    pub fn write_pretty(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())
    }

    pub fn mark_bevy_host_running(
        &mut self,
        updates: u64,
        handed_off_command_buffers: u64,
        output_composed: bool,
    ) {
        self.runtime.bevy_updates = updates;
        self.runtime.bevy_output_command_buffers_handed_to_host = handed_off_command_buffers;
        self.runtime.bevy_direct_queue_submissions = 0;
        if let Some(dependency) = self
            .dependencies
            .iter_mut()
            .find(|dependency| dependency.name == "Bevy")
        {
            dependency.state = if output_composed {
                "manual host resources and GPU output active"
            } else {
                "manual host resources active"
            }
            .into();
        }
        if let Some(adapter) = self
            .adapters
            .iter_mut()
            .find(|adapter| adapter.name == "Bevy world")
        {
            adapter.state = if output_composed {
                "manual shared-device output active"
            } else {
                "manual shared device active"
            }
            .into();
            adapter.detail = format!(
                "Lince owns the event loop, WGPU resources and queue submission; Bevy has completed {updates} manually driven empty-world updates without a Bevy window runner and handed {handed_off_command_buffers} output command buffers to the host; GPU output composed: {output_composed}"
            );
        }
        self.limitations
            .retain(|limitation| limitation != "Bevy manual rendering is not integrated yet.");
        self.limitations.retain(|limitation| {
            limitation
                != "The host currently proves only a human-runnable Lince-owned winit/wgpu panel and report surface."
        });
        self.limitations
            .retain(|limitation| !limitation.contains("Bevy render output"));
        if output_composed
            && !self
                .limitations
                .iter()
                .any(|limitation| limitation.contains("Bevy screenshot"))
        {
            self.limitations.push(
                "Bevy screenshot, GPU-readback and window-presentation finalization are bypassed with its stock render system; admitted versions need host-owned equivalents rather than direct Bevy queue submission.".into(),
            );
        } else if !output_composed {
            self.limitations.push(
                "Bevy render output is not composed into the host surface yet; this proves shared resource initialization and lifetime only.".into(),
            );
        }
    }

    pub fn record_host_redraw(&mut self) {
        self.runtime.host_redraw_events += 1;
    }

    pub fn record_host_frame(
        &mut self,
        elapsed: Duration,
        submitted_command_buffers: u64,
        submission_participants: Vec<String>,
    ) {
        let micros = elapsed.as_micros();
        self.runtime.host_frames_presented += 1;
        self.runtime.host_queue_submissions += 1;
        self.runtime.host_command_buffers_submitted += submitted_command_buffers;
        self.runtime.last_submission_participants = submission_participants;
        self.runtime.last_presented_frame_cpu_micros = micros;
        self.runtime.maximum_presented_frame_cpu_micros =
            self.runtime.maximum_presented_frame_cpu_micros.max(micros);
    }

    pub fn record_keyboard_input(&mut self) {
        self.runtime.keyboard_input_events += 1;
    }

    pub fn record_pointer_input(&mut self) {
        self.runtime.pointer_input_events += 1;
    }

    pub fn record_focus_event(&mut self) {
        self.runtime.focus_events += 1;
    }

    pub fn record_ime_event(&mut self) {
        self.runtime.ime_events += 1;
    }

    pub fn record_input_to_submit(&mut self, elapsed: Duration) {
        let micros = elapsed.as_micros();
        self.runtime.input_to_submit_samples += 1;
        self.runtime.last_input_to_submit_micros = micros;
        self.runtime.maximum_input_to_submit_micros =
            self.runtime.maximum_input_to_submit_micros.max(micros);
    }

    pub fn record_resize(&mut self) {
        self.runtime.resize_events += 1;
    }

    pub fn record_scale_factor_change(&mut self) {
        self.runtime.scale_factor_events += 1;
    }

    pub fn record_surface_recovery(&mut self, success: bool) {
        self.runtime.surface_recoveries += 1;
        if !success {
            self.runtime.surface_recovery_failures += 1;
        }
    }

    pub fn record_input_totals(
        &mut self,
        pointer_events: u64,
        keyboard_events: u64,
        focus_events: u64,
        ime_events: u64,
    ) {
        self.runtime.pointer_input_events = pointer_events;
        self.runtime.keyboard_input_events = keyboard_events;
        self.runtime.focus_events = focus_events;
        self.runtime.ime_events = ime_events;
    }

    pub fn record_input_evidence(&mut self, evidence: &InputEvidence) {
        self.runtime.pointer_input_events = evidence.pointer();
        self.runtime.keyboard_input_events = evidence.keyboard();
        self.runtime.focus_events = evidence.focus();
        self.runtime.ime_events = evidence.ime();
        self.runtime.normalized_input_events = evidence.accepted();
        self.runtime.rejected_input_events = evidence.rejected();
        self.runtime.last_input_target_local = evidence
            .last()
            .and_then(InputEnvelope::target_local_position)
            .map(|point| [point.x, point.y]);
        self.runtime.last_normalized_input = evidence.last().cloned();
        self.runtime.last_input_rejection = evidence.last_rejection().map(str::to_owned);
    }

    pub fn record_window_stress(&mut self, stress: &WindowStress) {
        self.runtime.window_stress = stress.facts();
    }

    pub fn record_teardown(
        &mut self,
        complete: bool,
        failures: u64,
        gpu_queue_idle: Option<bool>,
        external_slot: impl Into<String>,
        graceful_exit_requested: bool,
    ) {
        self.runtime.teardown_started = true;
        self.runtime.teardown_complete = complete;
        self.runtime.teardown_failures = failures;
        self.runtime.gpu_queue_idle_before_exit = gpu_queue_idle;
        self.runtime.external_slot_teardown = external_slot.into();
        self.runtime.graceful_exit_requested = graceful_exit_requested;
    }

    pub fn write_default(&self) -> Result<PathBuf, String> {
        let path = std::env::var_os("LINCE_INTERFACE_REPORT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from("target/interface/reports")
                    .join(format!("ownership-{}.json", unix_millis()))
            });
        self.write_pretty(&path)?;
        Ok(path)
    }

    pub fn panel_text(&self, last_export: Option<&str>) -> String {
        let failure = self.host.initialization_error.as_deref().unwrap_or("NONE");
        let export = last_export.unwrap_or("NOT EXPORTED");
        let bevy = self
            .adapters
            .iter()
            .find(|adapter| adapter.name == "Bevy world")
            .map_or("UNAVAILABLE", |adapter| adapter.state.as_str());
        let window_stress = if self.runtime.window_stress.requested_steps == 0 {
            "NOT STARTED".to_owned()
        } else if self.runtime.window_stress.complete {
            format!(
                "COMPLETE {}/{} CONVERGED",
                self.runtime.window_stress.converged_steps,
                self.runtime.window_stress.observed_steps
            )
        } else {
            format!(
                "RUNNING {}/{} OBSERVED",
                self.runtime.window_stress.observed_steps,
                self.runtime.window_stress.requested_steps
            )
        };

        format!(
            "LINCE INTERFACE LABORATORY\nNATIVE RENDER AND INPUT SEAM\n\nSTATUS             {}\nHOST EVENT LOOP    {}\nHOST GPU OWNER     {}\nFINAL COMPOSITOR   {}\nGPU                {}\nBACKEND            {}\nDEVICE TYPE        {}\nDRIVER             {}\nSURFACE            {}  {}X{}\nPRESENT MODE       {}\nSCALE FACTOR       {:.2}\nHOST FRAMES        {}\nBEVY UPDATES       {}\nINPUT P/K/F/IME    {}/{}/{}/{}\nNORMALIZED/REFUSED {}/{}\nLAST INPUT         {} -> {} #{}\nLAST INPUT-SUBMIT  {} US\nLAST CPU FRAME     {} US\nWINDOW STRESS      {}\nSURFACE RECOVERY   {}/{}\n\nRETAINED UI        LINCE OWNED\nBEVY WORLD         {}\nBIG SPACE          OPTIONAL COMPARISON\n\nINITIALIZATION     {}\nLAST REPORT        {}\n\nA  RUN AND EXPORT DEPENDENCY AUDIT\nD  RECREATE THE PRESENTATION SURFACE\nE  EXPORT MACHINE READABLE REPORT\nR  RUN RESIZE STRESS\nESC  EXIT",
            self.status.to_uppercase(),
            self.host.event_loop_owner.to_uppercase(),
            self.host.rendering_device_owner.to_uppercase(),
            self.host.final_compositor_owner.to_uppercase(),
            self.host.gpu_name.to_uppercase(),
            self.host.gpu_backend.to_uppercase(),
            self.host.gpu_device_type.to_uppercase(),
            self.host.gpu_driver.to_uppercase(),
            self.host.surface_format.to_uppercase(),
            self.host.surface_width,
            self.host.surface_height,
            self.host.present_mode.to_uppercase(),
            self.host.scale_factor,
            self.runtime.host_frames_presented,
            self.runtime.bevy_updates,
            self.runtime.pointer_input_events,
            self.runtime.keyboard_input_events,
            self.runtime.focus_events,
            self.runtime.ime_events,
            self.runtime.normalized_input_events,
            self.runtime.rejected_input_events,
            self.runtime
                .last_normalized_input
                .as_ref()
                .map_or("NONE", |input| input.event.kind()),
            self.runtime
                .last_normalized_input
                .as_ref()
                .map_or("NONE", |input| input.target.semantic_id.as_str()),
            self.runtime
                .last_normalized_input
                .as_ref()
                .map_or(0, |input| input.sequence),
            self.runtime.last_input_to_submit_micros,
            self.runtime.last_presented_frame_cpu_micros,
            window_stress,
            self.runtime.surface_recoveries,
            self.runtime.surface_recovery_failures,
            bevy.to_uppercase(),
            failure.to_uppercase(),
            export.to_uppercase(),
        )
    }
}

fn dependency_facts() -> Vec<DependencyFact> {
    vec![
        DependencyFact {
            name: "winit".into(),
            revision: "0.30.12".into(),
            role: "preferred host event loop and window".into(),
            state: "integrated".into(),
            license: "Apache-2.0".into(),
            gpu_family: None,
            raw_window_handle_family: Some("0.6".into()),
        },
        DependencyFact {
            name: "wgpu".into(),
            revision: "29.0.4".into(),
            role: "preferred host rendering device and compositor".into(),
            state: "integrated".into(),
            license: "MIT OR Apache-2.0".into(),
            gpu_family: Some("wgpu 29 / wgpu-core 29 / wgpu-hal 29".into()),
            raw_window_handle_family: Some("0.6.2".into()),
        },
        DependencyFact {
            name: "glyphon".into(),
            revision: "0.11.0".into(),
            role: "temporary laboratory text surface".into(),
            state: "integrated".into(),
            license: "MIT OR Apache-2.0 OR Zlib".into(),
            gpu_family: Some("wgpu 29".into()),
            raw_window_handle_family: None,
        },
        DependencyFact {
            name: "Bevy".into(),
            revision: "0.19.1".into(),
            role: "manual retained world renderer".into(),
            state: "source graph compiled; manual rendering not integrated".into(),
            license: "MIT OR Apache-2.0".into(),
            gpu_family: Some("wgpu 29.0.4".into()),
            raw_window_handle_family: Some("0.6 through the renderer graph".into()),
        },
        DependencyFact {
            name: "Lato Regular".into(),
            revision: "repository asset".into(),
            role: "laboratory panel typography".into(),
            state: "integrated".into(),
            license: "SIL Open Font License 1.1".into(),
            gpu_family: None,
            raw_window_handle_family: None,
        },
    ]
}

fn adapter_facts() -> Vec<AdapterFact> {
    vec![
        AdapterFact {
            name: "Lince-owned host".into(),
            state: "running".into(),
            owner: "Lince".into(),
            detail: "winit event loop, wgpu device and queue, swapchain, final clear/text pass and versioned normalized input".into(),
        },
        AdapterFact {
            name: "Bevy world".into(),
            state: "source graph compiled; not integrated".into(),
            owner: "Lince frame coordinator".into(),
            detail: "manual render resources not exercised".into(),
        },
    ]
}

pub fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

pub fn raw_git_revision() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|revision| revision.trim().to_owned())
        .filter(|revision| !revision.is_empty())
        .unwrap_or_else(|| "unavailable".into())
}

pub fn git_dirty() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| !output.stdout.is_empty())
}

pub fn git_revision() -> String {
    let revision = raw_git_revision();
    if revision != "unavailable" && git_dirty() {
        format!("{revision}-dirty")
    } else {
        revision
    }
}

pub fn source_fingerprint() -> String {
    let paths = Command::new("git")
        .args(["ls-files", "-co", "--exclude-standard", "-z"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| output.stdout)
        .unwrap_or_default();
    let mut hasher = Sha256::new();
    for path in paths
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = String::from_utf8_lossy(path);
        if !(path.starts_with("crates/interface/")
            || path.starts_with("crates/desktop/")
            || path.starts_with("scripts/interface/")
            || matches!(
                path.as_ref(),
                "Cargo.lock" | "Cargo.toml" | "flake.nix" | "mise.toml"
            ))
        {
            continue;
        }
        hasher.update(path.as_bytes());
        hasher.update([0]);
        if let Ok(bytes) = fs::read(path.as_ref()) {
            hasher.update(bytes);
        }
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{
        InputModifiers, InputSource, InputSurface, InputTarget, NormalizedInput, Point,
    };
    use std::collections::BTreeSet;

    fn host() -> HostFacts {
        HostFacts {
            operating_system: "linux".into(),
            architecture: "x86_64".into(),
            event_loop_owner: "lince-winit-host".into(),
            rendering_device_owner: "lince-wgpu-host".into(),
            final_compositor_owner: "lince-wgpu-host".into(),
            gpu_name: "fixture gpu".into(),
            gpu_backend: "vulkan".into(),
            gpu_device_type: "integrated".into(),
            gpu_driver: "fixture".into(),
            gpu_driver_info: "fixture driver".into(),
            surface_format: "bgra8unormsrgb".into(),
            present_mode: "fifo".into(),
            scale_factor: 1.0,
            surface_width: 1280,
            surface_height: 800,
            initialization_error: None,
        }
    }

    #[test]
    fn report_names_each_dependency_once() {
        let report = LaboratoryReport::ownership_preflight(host());
        let names = report
            .dependencies
            .iter()
            .map(|dependency| dependency.name.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(names.len(), report.dependencies.len());
        assert_eq!(report.schema_version, REPORT_SCHEMA_VERSION);
        assert_eq!(report.status, "running-incomplete");
    }

    #[test]
    fn panel_is_honest_about_unintegrated_adapters() {
        let panel = LaboratoryReport::ownership_preflight(host()).panel_text(None);

        assert!(panel.contains("RETAINED UI        LINCE OWNED"));
        assert!(panel.contains("BEVY WORLD         SOURCE GRAPH COMPILED; NOT INTEGRATED"));
        assert!(panel.contains("A  RUN AND EXPORT DEPENDENCY AUDIT"));
        assert!(panel.contains("LAST REPORT        NOT EXPORTED"));
    }

    #[test]
    fn runtime_facts_distinguish_updates_compositions_and_presented_frames() {
        let mut report = LaboratoryReport::ownership_preflight(host());
        report.mark_bevy_host_running(2, 2, true);
        report.record_host_redraw();
        report.record_host_frame(
            Duration::from_micros(320),
            2,
            vec!["bevy-world".into(), "lince-compositor".into()],
        );
        report.record_host_frame(
            Duration::from_micros(640),
            2,
            vec!["bevy-world".into(), "lince-compositor".into()],
        );

        assert_eq!(report.runtime.bevy_updates, 2);
        assert_eq!(report.runtime.bevy_output_command_buffers_handed_to_host, 2);
        assert_eq!(report.runtime.bevy_direct_queue_submissions, 0);
        assert_eq!(report.runtime.host_frames_presented, 2);
        assert_eq!(report.runtime.host_queue_submissions, 2);
        assert_eq!(report.runtime.host_command_buffers_submitted, 4);
        assert_eq!(
            report.runtime.last_submission_participants,
            ["bevy-world", "lince-compositor"]
        );
        assert_eq!(report.runtime.last_presented_frame_cpu_micros, 640);
        assert_eq!(report.runtime.maximum_presented_frame_cpu_micros, 640);
    }

    #[test]
    fn runtime_facts_keep_input_categories_and_submit_latency_distinct() {
        let mut report = LaboratoryReport::ownership_preflight(host());
        report.record_pointer_input();
        report.record_keyboard_input();
        report.record_focus_event();
        report.record_ime_event();
        report.record_input_to_submit(Duration::from_micros(240));
        report.record_input_to_submit(Duration::from_micros(180));

        assert_eq!(report.runtime.pointer_input_events, 1);
        assert_eq!(report.runtime.keyboard_input_events, 1);
        assert_eq!(report.runtime.focus_events, 1);
        assert_eq!(report.runtime.ime_events, 1);
        assert_eq!(report.runtime.input_to_submit_samples, 2);
        assert_eq!(report.runtime.last_input_to_submit_micros, 180);
        assert_eq!(report.runtime.maximum_input_to_submit_micros, 240);
    }

    #[test]
    fn report_preserves_normalized_input_and_target_local_coordinates() {
        let mut report = LaboratoryReport::ownership_preflight(host());
        let mut evidence = InputEvidence::default();
        let surface = InputSurface::new("box-main", 1600, 1000, 2.0);
        let target = InputTarget::full_surface("record-card", "lince-winit", &surface);
        evidence
            .record(InputEnvelope::new(
                1,
                InputSource::LinceWinit,
                surface,
                target,
                NormalizedInput::PointerMoved {
                    surface_physical: Point::new(300.0, 120.0),
                    buttons: Vec::new(),
                    modifiers: InputModifiers::default(),
                },
            ))
            .expect("valid normalized input");

        report.record_input_evidence(&evidence);

        assert_eq!(report.runtime.normalized_input_events, 1);
        assert_eq!(report.runtime.rejected_input_events, 0);
        assert_eq!(report.runtime.pointer_input_events, 1);
        assert_eq!(report.runtime.last_input_target_local, Some([150.0, 60.0]));
        assert_eq!(
            report
                .runtime
                .last_normalized_input
                .as_ref()
                .map(|input| input.target.semantic_id.as_str()),
            Some("record-card")
        );
    }
}
