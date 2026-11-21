// non-lean-yet
// Scope: struct Time, struct Reading, struct System, struct Counter.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 425.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   579.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Result, error};

#[derive(Default, Deserialize, Serialize)]
struct Time {
    count: u64,
    total_us: u64,
    maximum_us: u64,
    #[serde(default, skip_serializing)]
    buckets: Option<Vec<u64>>,
    #[serde(skip_deserializing)]
    p50_upper_us: Option<u64>,
    #[serde(skip_deserializing)]
    p95_upper_us: Option<u64>,
    #[serde(skip_deserializing)]
    p99_upper_us: Option<u64>,
}

impl Time {
    fn add(&mut self, sample: Self) {
        if self.count == 0 {
            self.buckets = sample.buckets;
        } else if let (Some(left), Some(right)) = (&mut self.buckets, sample.buckets)
            && left.len() == right.len()
        {
            for (left, right) in left.iter_mut().zip(right) {
                *left = left.saturating_add(right);
            }
        } else {
            self.buckets = None;
        }
        self.count = self.count.saturating_add(sample.count);
        self.total_us = self.total_us.saturating_add(sample.total_us);
        self.maximum_us = self.maximum_us.max(sample.maximum_us);
    }

    fn interval(&mut self, start: u64, end: u64) {
        if let Some(elapsed) = end.checked_sub(start) {
            self.add(Self {
                count: 1,
                total_us: elapsed,
                maximum_us: elapsed,
                ..Default::default()
            });
        }
    }

    fn finish(&mut self, limits: &[u64]) {
        let Some(buckets) = self.buckets.take().filter(|buckets| {
            buckets.len() == limits.len()
                && self.count > 0
                && buckets.iter().map(|count| u128::from(*count)).sum::<u128>()
                    == u128::from(self.count)
        }) else {
            return;
        };
        let percentile = |percent: u128| {
            let rank = (u128::from(self.count) * percent).div_ceil(100);
            let mut seen = 0;
            limits.iter().zip(&buckets).find_map(|(limit, count)| {
                seen += u128::from(*count);
                (seen >= rank).then_some((*limit).min(self.maximum_us))
            })
        };
        self.p50_upper_us = percentile(50);
        self.p95_upper_us = percentile(95);
        self.p99_upper_us = percentile(99);
    }
}

#[derive(Deserialize)]
struct Reading {
    stage: String,
    kind: String,
    #[serde(flatten)]
    time: Time,
}

#[derive(Deserialize)]
struct System {
    system: String,
    exclusive_owner_cpu: Time,
    unattributed_thread_samples: u64,
}

#[derive(Deserialize)]
struct Counter {
    counter: String,
    count: u64,
}

#[derive(Deserialize)]
struct InputPresentation {
    stage: String,
    frame: u64,
    output: Option<u64>,
    origin_us: u64,
    observed_us: u64,
}

#[derive(Default, Deserialize, Eq, PartialEq, Serialize)]
struct Identity {
    capture_started_unix_ns: Option<String>,
    revision: Option<String>,
    optimized: Option<bool>,
}

#[derive(Deserialize)]
struct Report {
    schema_version: u32,
    process: u32,
    #[serde(flatten)]
    identity: Identity,
    histogram_limits_us: Vec<u64>,
    workload: Option<String>,
    phase: String,
    sequence: u64,
    window_started_us: u64,
    window_ended_us: u64,
    final_report: bool,
    dropped_reports: u64,
    dropped_system_samples: u64,
    dropped_span_trace: u64,
    dropped_input_trace: u64,
    coverage: Vec<Value>,
    counters: Vec<Counter>,
    stages: Vec<Reading>,
    systems: Vec<System>,
    #[serde(default)]
    input_trace: Vec<InputPresentation>,
    #[serde(default)]
    park_frames: Vec<Value>,
    dropped_park_frames: Option<u64>,
}

#[derive(Default, Serialize)]
struct Phase {
    windows: u64,
    final_windows: u64,
    elapsed_us: u64,
    dropped_system_samples: u64,
    dropped_span_trace: u64,
    dropped_input_trace: u64,
    unattributed_system_thread_samples: u64,
    stages_by_timing_kind: BTreeMap<String, BTreeMap<String, Time>>,
    counters: BTreeMap<String, u64>,
    coverage: BTreeMap<String, u64>,
    observer_and_schedule_envelopes: BTreeMap<String, Time>,
    ranked_exclusive_system_cpu: Vec<(String, Time)>,
    #[serde(skip)]
    systems: BTreeMap<String, Time>,
}

#[derive(Default)]
struct Capture {
    process: Option<u32>,
    identity: Identity,
    histogram_limits_us: Vec<u64>,
    last_sequence: Option<u64>,
    previous_end: Option<u64>,
    final_report: bool,
    gaps: u64,
    overlapping_windows: u64,
    dropped_reports: u64,
    dropped_park_frames: Option<u64>,
    frames: FrameSummary,
    input_presentations: Vec<InputPresentation>,
    workloads: BTreeMap<String, BTreeMap<String, Phase>>,
}

impl Capture {
    fn add(&mut self, report: Report) -> Result<()> {
        if self
            .process
            .is_some_and(|process| process != report.process)
        {
            return Err("The phase log contains multiple processes. Select one with --pid.".into());
        }
        if report.schema_version != 2 || report.window_ended_us < report.window_started_us {
            return Err("Unknown phase schema or invalid capture window".into());
        }
        if self.process.is_some() {
            if self.identity != report.identity
                || self.histogram_limits_us != report.histogram_limits_us
            {
                return Err(
                    "Capture identity or histogram boundaries changed between windows".into(),
                );
            }
        } else {
            if report.histogram_limits_us.is_empty() || !report.histogram_limits_us.is_sorted() {
                return Err("Invalid histogram boundaries".into());
            }
            self.identity = report.identity;
            self.histogram_limits_us = report.histogram_limits_us;
        }
        if let Some(last) = self.last_sequence {
            if report.sequence <= last || self.final_report {
                return Err("Duplicate or out-of-order phase windows".into());
            }
            self.gaps += report.sequence - last - 1;
        } else {
            self.gaps = report
                .sequence
                .checked_sub(1)
                .ok_or("Invalid initial phase window sequence")?;
        }
        if self
            .previous_end
            .is_some_and(|end| end > report.window_started_us)
        {
            self.overlapping_windows += 1;
        }
        self.process = Some(report.process);
        self.previous_end = Some(report.window_ended_us);
        self.last_sequence = Some(report.sequence);
        self.final_report = report.final_report;
        self.dropped_reports = self.dropped_reports.max(report.dropped_reports);
        if let Some(dropped) = report.dropped_park_frames {
            let total = self.dropped_park_frames.get_or_insert(0);
            *total = total.saturating_add(dropped);
        }
        for frame in report.park_frames {
            self.frames.add(frame)?;
        }
        self.input_presentations
            .extend(report.input_trace.into_iter().filter(|input| {
                input.frame != 0
                    && input.output.is_some()
                    && matches!(input.stage.as_str(), "presented-drm" | "presented-observed")
            }));
        let phase = self
            .workloads
            .entry(report.workload.unwrap_or_else(|| "unlabeled".into()))
            .or_default()
            .entry(report.phase)
            .or_default();
        phase.windows += 1;
        phase.final_windows += u64::from(report.final_report);
        phase.elapsed_us += report.window_ended_us - report.window_started_us;
        phase.dropped_system_samples += report.dropped_system_samples;
        phase.dropped_span_trace += report.dropped_span_trace;
        phase.dropped_input_trace += report.dropped_input_trace;
        for reading in report.stages {
            phase
                .stages_by_timing_kind
                .entry(reading.kind)
                .or_default()
                .entry(reading.stage)
                .or_default()
                .add(reading.time);
        }
        for counter in report.counters {
            *phase.counters.entry(counter.counter).or_default() += counter.count;
        }
        for entry in report.coverage {
            *phase.coverage.entry(entry.to_string()).or_default() += 1;
        }
        for system in report.systems {
            phase.unattributed_system_thread_samples += system.unattributed_thread_samples;
            let target = if system.system.ends_with("phase_metrics::run")
                || system.system.ends_with("::run_render_schedule")
            {
                &mut phase.observer_and_schedule_envelopes
            } else {
                &mut phase.systems
            };
            target
                .entry(system.system)
                .or_default()
                .add(system.exclusive_owner_cpu);
        }
        Ok(())
    }
}

fn rows(path: &Path, marker: &str, mut visit: impl FnMut(&str) -> Result<()>) -> Result<()> {
    let file = File::open(path).map_err(error)?;
    let mut reader = BufReader::new(file);
    let mut bytes = Vec::new();
    let mut line = 0;
    loop {
        bytes.clear();
        if reader
            .by_ref()
            .take(16 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(error)?
            == 0
        {
            return Ok(());
        }
        line += 1;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(format!("{} line {line} exceeds 16 MiB", path.display()));
        }
        let text = String::from_utf8_lossy(&bytes);
        if let Some((_, payload)) = text.split_once(marker) {
            visit(payload)
                .map_err(|failure| format!("{} line {line}: {failure}", path.display()))?;
        }
    }
}

#[derive(Default, Serialize)]
struct Output {
    composition_observations: u64,
    native_image_available: u64,
    sampled_native_render_observations: u64,
    drm_presentations: u64,
    drm_presentations_with_sampled_native_render: u64,
    dispatch_clock_observations: u64,
    gles_queries_queued: u64,
    gles_queries_read_back: u64,
    gles_command_samples: u64,
    gles_command_interval_ns_total: u128,
    gles_readback_unavailable: u64,
    host_elapsed: Time,
    host_owner_cpu: Time,
    composition_to_fence_ready_queue_elapsed: Time,
    composition_to_drm_elapsed: Time,
    gles_query_readback_age: Time,
}

#[derive(Default, Serialize)]
struct InputLinks {
    observed_input_presentations: u64,
    without_matching_sampled_image: u64,
    origin_to_sampled_presentation: Time,
}

#[derive(Deserialize)]
struct Link {
    process: u32,
    sample: Value,
}

#[derive(Clone, Default, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct RenderScope {
    workload: Option<String>,
    phase: Option<String>,
    pixel_only: Option<bool>,
}

#[derive(Default, Serialize)]
struct GpuTiming {
    count: u64,
    total: f64,
    maximum: f64,
}

#[derive(Default)]
struct FrameSummary {
    starts: BTreeMap<u64, RenderScope>,
    duplicate_starts: u64,
    reference: BTreeSet<u64>,
    counts: BTreeMap<String, u64>,
    outputs: BTreeMap<u64, Output>,
    intervals: BTreeMap<String, Time>,
    native_gpu: BTreeMap<RenderScope, BTreeMap<String, GpuTiming>>,
    completed_gpu_frames: BTreeSet<u64>,
    native_gpu_readback_age: Time,
    native_gpu_truncated: u64,
    gles_supported: Option<bool>,
    sampled_presentations: BTreeSet<(u64, u64, u64, bool)>,
}

impl FrameSummary {
    fn add(&mut self, sample: Value) -> Result<()> {
        let event = sample["event"].as_str().ok_or("Missing frame link event")?;
        let at = sample["monotonic_us"].as_u64();
        let render = sample["render"].as_u64();
        *self.counts.entry(event.to_owned()).or_default() += 1;
        if let Some(render) = render {
            if event == "render-start" {
                if let std::collections::btree_map::Entry::Vacant(entry) = self.starts.entry(render)
                {
                    entry.insert(RenderScope {
                        workload: sample["workload"].as_str().map(str::to_owned),
                        phase: sample["phase"].as_str().map(str::to_owned),
                        pixel_only: sample["pixel_only"].as_bool(),
                    });
                } else {
                    self.duplicate_starts += 1;
                }
            } else {
                self.reference.insert(render);
            }
        }
        match event {
            "native-extraction" | "native-export" | "native-import" => {
                if let Some((start, end)) = sample["started_us"].as_u64().zip(at) {
                    self.intervals
                        .entry(event.to_owned())
                        .or_default()
                        .interval(start, end);
                }
            }
            "native-gpu-readback" => {
                self.native_gpu_truncated += u64::from(sample["truncated"] == true);
                if let Some(age) = sample["age_us"].as_u64() {
                    self.native_gpu_readback_age.interval(0, age);
                }
                if let Some(samples) = sample["samples"].as_array() {
                    let scope = render
                        .and_then(|id| self.starts.get(&id))
                        .cloned()
                        .unwrap_or_default();
                    for value in samples {
                        if let (Some(path), Some(unit), Some(duration)) = (
                            value["path"].as_str(),
                            value["unit"].as_str(),
                            value["value"].as_f64(),
                        ) {
                            let aggregate = self
                                .native_gpu
                                .entry(scope.clone())
                                .or_default()
                                .entry(format!("{path} ({unit})"))
                                .or_default();
                            aggregate.count += 1;
                            aggregate.total += duration;
                            aggregate.maximum = aggregate.maximum.max(duration);
                            self.completed_gpu_frames.extend(render);
                        }
                    }
                }
            }
            "gles-timestamp-capability" => self.gles_supported = sample["supported"].as_bool(),
            _ => {}
        }
        if let Some(id) = sample["output"].as_u64() {
            let output = self.outputs.entry(id).or_default();
            let start = sample["composition_started_us"].as_u64();
            let sampled = sample["sampled_native_render"].as_u64();
            self.reference.extend(sampled);
            if event == "presented"
                && sampled.is_some()
                && sampled == render
                && let Some(frame) = sample["frame"].as_u64()
                && let Some(at) = at
                && let Some(hardware) = sample["hardware_clock"].as_bool()
            {
                self.sampled_presentations.insert((frame, id, at, hardware));
            }
            match event {
                "gles-host-return" => {
                    output.composition_observations += 1;
                    output.native_image_available +=
                        u64::from(sample["native_image_available"] == true);
                    output.sampled_native_render_observations += u64::from(sampled.is_some());
                    output.gles_queries_queued +=
                        u64::from(sample["hardware_query_queued"] == true);
                    if let Some((start, end)) = sample["started_us"].as_u64().zip(at) {
                        output.host_elapsed.interval(start, end);
                    }
                    if let Some(cpu) = sample["owner_cpu_us"].as_u64() {
                        output.host_owner_cpu.interval(0, cpu);
                    }
                }
                "fence-ready-queue-attempt" => {
                    if let Some((start, end)) = start.zip(at) {
                        output
                            .composition_to_fence_ready_queue_elapsed
                            .interval(start, end);
                    }
                }
                "presented" if sample["hardware_clock"] == true => {
                    output.drm_presentations += 1;
                    output.drm_presentations_with_sampled_native_render +=
                        u64::from(sampled.is_some());
                    if let Some((start, end)) = start.zip(at) {
                        output.composition_to_drm_elapsed.interval(start, end);
                    }
                }
                "presented" => output.dispatch_clock_observations += 1,
                "gles-timestamp-readback" => {
                    output.gles_queries_read_back += 1;
                    if let Some(age) = sample["readback_age_us"].as_u64() {
                        output.gles_query_readback_age.interval(0, age);
                    }
                    if let Some(duration) = sample["command_interval_ns"].as_u64() {
                        output.gles_command_samples += 1;
                        output.gles_command_interval_ns_total += u128::from(duration);
                    } else {
                        output.gles_readback_unavailable += 1;
                    }
                }
                "gles-timestamp-skipped" | "gles-timestamp-unavailable" => {
                    output.gles_readback_unavailable += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn report(self, input: &[InputPresentation]) -> Option<Value> {
        if self.counts.is_empty() {
            return None;
        }
        let native_gpu: Vec<_> = self
            .native_gpu
            .into_iter()
            .map(|(scope, passes)| json!({"source":scope, "passes":passes}))
            .collect();
        let mut input_links = BTreeMap::<u64, BTreeMap<&'static str, InputLinks>>::new();
        for input in input {
            let Some(output) = input.output else { continue };
            let hardware = input.stage == "presented-drm";
            let clock = if hardware { "drm" } else { "dispatch" };
            let links = input_links
                .entry(output)
                .or_default()
                .entry(clock)
                .or_default();
            links.observed_input_presentations += 1;
            if self.sampled_presentations.contains(&(
                input.frame,
                output,
                input.observed_us,
                hardware,
            )) {
                links
                    .origin_to_sampled_presentation
                    .interval(input.origin_us, input.observed_us);
            } else {
                links.without_matching_sampled_image += 1;
            }
        }
        Some(json!({
            "event_counts":self.counts, "native_renders_started":self.starts.len(),
            "duplicate_render_starts":self.duplicate_starts,
            "native_renders_with_gpu_samples":self.completed_gpu_frames.len(),
            "referenced_render_ids_without_start":self.reference.iter().filter(|id| !self.starts.contains_key(*id)).count(),
            "native_host_elapsed":self.intervals, "native_gpu_by_source":native_gpu,
            "native_gpu_readback_age":self.native_gpu_readback_age,
            "native_gpu_truncated_readbacks":self.native_gpu_truncated,
            "gles_hardware_timestamps_supported":self.gles_supported,
            "outputs":self.outputs,
            "native_input_links":input_links,
            "coverage":"A retained render can appear in multiple outputs or compositions. Times with zero observations are unavailable. The input join observes a sampled frame, with no independent visible-pixel oracle. Host elapsed, queue age and GPU command intervals can overlap."
        }))
    }
}

fn frame_summary(path: &Path, pid: u32, input: &[InputPresentation]) -> Result<Value> {
    let mut summary = FrameSummary::default();
    rows(path, "Park frame link ", |payload| {
        let link: Link = serde_json::from_str(payload).map_err(error)?;
        if link.process == pid {
            summary.add(link.sample)?;
        }
        Ok(())
    })?;
    summary
        .report(input)
        .ok_or_else(|| "No frame link records for the selected process".into())
}

pub(super) fn analyze(arguments: &[OsString]) -> Result<()> {
    if arguments.is_empty() || arguments[0] == "--help" {
        println!("cargo xtask park validate attribute PHASES_LOG [--pid PID] [--frames FRAME_LOG]");
        return Ok(());
    }
    if !arguments.len().is_multiple_of(2) {
        return Err("Use one phase log followed by optional --pid and --frames pairs".into());
    }
    let mut pid = None;
    let mut frames = None;
    for pair in arguments[1..].as_chunks::<2>().0 {
        if pair[0] == "--pid" && pid.is_none() {
            pid = Some(
                pair[1]
                    .to_str()
                    .and_then(|value| value.parse::<u32>().ok())
                    .filter(|pid| *pid != 0)
                    .ok_or("Invalid phase PID")?,
            );
        } else if pair[0] == "--frames" && frames.is_none() {
            frames = Some(Path::new(&pair[1]));
        } else {
            return Err("Unknown or duplicate attribution option".into());
        }
    }
    let mut capture = Capture::default();
    rows(Path::new(&arguments[0]), "Lince performance ", |payload| {
        if !payload.trim_start().starts_with('{') {
            return Ok(());
        }
        let report: Report = serde_json::from_str(payload).map_err(error)?;
        if pid.is_none_or(|pid| pid == report.process) {
            capture.add(report)?;
        }
        Ok(())
    })?;
    let process = capture
        .process
        .ok_or("No phase records for the selected process")?;
    for phase in capture
        .workloads
        .values_mut()
        .flat_map(BTreeMap::values_mut)
    {
        let histograms = phase
            .stages_by_timing_kind
            .values_mut()
            .flat_map(BTreeMap::values_mut)
            .chain(phase.observer_and_schedule_envelopes.values_mut())
            .chain(phase.systems.values_mut());
        for timing in histograms {
            timing.finish(&capture.histogram_limits_us);
        }
        phase.ranked_exclusive_system_cpu =
            std::mem::take(&mut phase.systems).into_iter().collect();
        phase
            .ranked_exclusive_system_cpu
            .sort_by(|(left_name, left), (right_name, right)| {
                right
                    .total_us
                    .cmp(&left.total_us)
                    .then_with(|| left_name.cmp(right_name))
            });
    }
    let combined = capture.frames.report(&capture.input_presentations);
    if combined.is_some() && frames.is_some() {
        return Err("The phase log already contains frame links. Omit --frames.".into());
    }
    let frames = match frames {
        Some(path) => Some(frame_summary(path, process, &capture.input_presentations)?),
        None => combined,
    };
    let report = json!({
        "schema":"park-attribution-v1", "process":process, "identity":capture.identity,
        "unobserved_sequence_windows":capture.gaps, "overlapping_windows":capture.overlapping_windows,
        "final_report_observed":capture.final_report,
        "reported_dropped_reports":capture.dropped_reports,
        "dropped_park_frames":capture.dropped_park_frames,
        "workloads":capture.workloads, "frame_links":frames,
        "scope":"Rankings are per-phase exclusive owner CPU. Inclusive elapsed and wait stages are separate and overlap. Percentiles are merged histogram upper bounds. Frame links cover the capture. A named workload and matched build and capture conditions are required for before/after conclusions. Recording itself adds work."
    });
    println!("{}", serde_json::to_string_pretty(&report).map_err(error)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asynchronous_passes_keep_the_originating_render_phase() {
        let mut frames = FrameSummary::default();
        frames
            .add(json!({"event":"render-start", "render":1,
            "workload":"mixed", "phase":"gpu-full", "pixel_only":false}))
            .unwrap();
        frames
            .add(json!({"event":"render-start", "render":2,
            "workload":"mixed", "phase":"gpu-pixel", "pixel_only":true}))
            .unwrap();
        frames
            .add(json!({"event":"native-gpu-readback", "render":1,
            "samples":[{"path":"ui/elapsed_gpu", "unit":"ms", "value":0.25}]}))
            .unwrap();
        frames
            .add(json!({"event":"native-gpu-readback", "render":2,
            "samples":[{"path":"ui/elapsed_gpu", "unit":"ms", "value":0.1}]}))
            .unwrap();
        let report = frames.report(&[]).unwrap();
        assert_eq!(
            report["native_gpu_by_source"][0]["source"]["phase"],
            "gpu-full"
        );
        assert_eq!(
            report["native_gpu_by_source"][0]["passes"]["ui/elapsed_gpu (ms)"]["total"],
            0.25
        );
        assert_eq!(
            report["native_gpu_by_source"][1]["source"]["phase"],
            "gpu-pixel"
        );
    }

    #[test]
    fn merged_tails_use_all_samples_and_require_complete_buckets() {
        let fast = || Time {
            count: 96,
            total_us: 960,
            maximum_us: 10,
            buckets: Some(vec![96, 0]),
            ..Default::default()
        };
        let slow = || Time {
            count: 4,
            total_us: 400,
            maximum_us: 100,
            buckets: Some(vec![0, 4]),
            ..Default::default()
        };
        let mut merged = Time::default();
        merged.add(fast());
        merged.add(slow());
        merged.finish(&[10, u64::MAX]);
        assert_eq!(merged.count, 100);
        assert_eq!(merged.p95_upper_us, Some(10));
        assert_eq!(merged.p99_upper_us, Some(100));

        let mut incomplete = fast();
        incomplete.add(Time {
            buckets: None,
            ..slow()
        });
        incomplete.finish(&[10, u64::MAX]);
        assert_eq!(incomplete.total_us, 1360);
        assert_eq!(incomplete.p95_upper_us, None);
    }

    #[test]
    fn input_join_requires_exact_output_clock_timestamp_and_sampled_image() {
        let mut frames = FrameSummary::default();
        for (frame, native_image) in [(10, 5), (11, 4)] {
            frames
                .add(json!({"event":"presented", "render":5,
                "frame":frame, "output":2, "monotonic_us":1100, "hardware_clock":true,
                "sampled_native_render":native_image}))
                .unwrap();
        }
        let samples = [
            (10, 2, 1100, "presented-drm"),
            (11, 2, 1100, "presented-drm"),
            (10, 3, 1100, "presented-drm"),
            (10, 2, 1101, "presented-drm"),
            (10, 2, 1100, "presented-observed"),
        ]
        .map(|(frame, output, observed_us, stage)| InputPresentation {
            stage: stage.into(),
            frame,
            output: Some(output),
            origin_us: 100,
            observed_us,
        });
        let report = frames.report(&samples).unwrap();
        let links = &report["native_input_links"];
        assert_eq!(
            links["2"]["drm"]["origin_to_sampled_presentation"]["count"],
            1
        );
        assert_eq!(
            links["2"]["drm"]["origin_to_sampled_presentation"]["total_us"],
            1000
        );
        assert_eq!(links["2"]["drm"]["without_matching_sampled_image"], 2);
        assert_eq!(links["3"]["drm"]["without_matching_sampled_image"], 1);
        assert_eq!(links["2"]["dispatch"]["without_matching_sampled_image"], 1);
    }
}
