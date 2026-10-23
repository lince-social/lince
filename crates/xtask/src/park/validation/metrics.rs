use super::{Result, error};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::Path,
};

#[derive(Default)]
struct Samples {
    windows: BTreeMap<u64, Vec<(u64, u64, f64)>>,
    stages: BTreeMap<String, Vec<(f64, f64)>>,
    scanouts: BTreeMap<u64, Vec<(f64, f64, bool)>>,
}

fn parse(log: &str) -> Samples {
    let mut samples = Samples::default();
    for line in log.lines() {
        if let Some(line) = line.strip_prefix("Park app-presentation: ") {
            let fields: Vec<_> = line.split_whitespace().collect();
            if let [window, node, version, time] = fields.as_slice()
                && let (Ok(window), Ok(node), Ok(version), Ok(time)) = (
                    window.parse(),
                    node.parse(),
                    version.parse(),
                    time.parse::<f64>(),
                )
                && time.is_finite()
            {
                samples
                    .windows
                    .entry(window)
                    .or_default()
                    .push((node, version, time));
            }
        } else if let Some(line) = line.strip_prefix("Park scanout: ") {
            let fields: Vec<_> = line.split_whitespace().collect();
            if let [output, presented, observed, clock @ ("drm" | "dispatch")] = fields.as_slice()
                && let (Ok(output), Ok(presented), Ok(observed)) = (
                    output.parse(), presented.parse::<f64>(), observed.parse::<f64>(),
                )
                && presented.is_finite() && presented >= 0.0 && observed.is_finite()
            {
                samples.scanouts.entry(output).or_default().push((presented, observed, *clock == "drm"));
            }
        } else if let Some(line) = line.strip_prefix("Park frame ") {
            let fields: Vec<_> = line.split_whitespace().collect();
            if let [stage, elapsed, "ms", "at", time] = fields.as_slice()
                && let (Ok(elapsed), Ok(time)) = (elapsed.parse::<f64>(), time.parse::<f64>())
                && elapsed.is_finite()
                && elapsed >= 0.0
                && time.is_finite()
            {
                samples
                    .stages
                    .entry(stage.trim_end_matches(':').into())
                    .or_default()
                    .push((time, elapsed));
            }
        }
    }
    samples
}

fn percentile(values: &[f64], quantile: f64) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted
        .get(((sorted.len() as f64 * quantile).ceil() as usize).saturating_sub(1))
        .copied()
}

fn summarize(
    samples: Samples,
    warmup: f64,
    seconds: f64,
    minimum_fps: f64,
    surface: Option<u64>,
) -> Result<Value> {
    let selected = surface.or_else(|| samples.windows.iter().max_by_key(|(_, events)| {
        events.iter().map(|(node, version, _)| (*node, *version)).collect::<BTreeSet<_>>().len()
    }).map(|(id, _)| *id)).ok_or("No application presentation traces; enable LINCE_PARK_TRACE_FRAMES=1")?;
    let selected_events = samples.windows.get(&selected).ok_or("Requested application surface was not present")?;
    let first = selected_events
        .iter()
        .map(|(_, _, time)| *time)
        .min_by(f64::total_cmp)
        .ok_or("No application presentation traces; enable LINCE_PARK_TRACE_FRAMES=1")?;
    let last = selected_events
        .iter()
        .map(|(_, _, time)| *time)
        .max_by(f64::total_cmp)
        .unwrap();
    let start = first + warmup;
    let end = start + seconds;
    if last + 0.001 < end {
        return Err("Trace does not cover the requested warmup and measurement duration".into());
    }
    let mut windows = BTreeMap::new();
    for (id, events) in samples.windows {
        let mut seen = BTreeSet::new();
        let mut times = BTreeSet::new();
        for (node, version, time) in events {
            if seen.insert((node, version)) && time >= start && time < end {
                times.insert((time * 1_000_000.0).round() as u64);
            }
        }
        let times: Vec<_> = times.into_iter().collect();
        let intervals: Vec<_> = times
            .windows(2)
            .map(|pair| (pair[1] - pair[0]) as f64 / 1000.0)
            .collect();
        windows.insert(id, json!({"fresh_frames": times.len(), "fresh_fps": times.len() as f64 / seconds,
            "interval_p95_ms": percentile(&intervals, 0.95), "maximum_interval_ms": percentile(&intervals, 1.0),
            "stalls_over_100_ms": intervals.iter().filter(|elapsed| **elapsed > 100.0).count()}));
    }
    let selected_window = windows
        .get(&selected)
        .ok_or("Requested application surface was not present")?;
    let fps = selected_window["fresh_fps"].as_f64().unwrap();
    let p95 = selected_window["interval_p95_ms"].as_f64();
    let passed = fps >= minimum_fps
        && p95.is_some_and(|p95| p95 <= 25.0)
        && selected_window["stalls_over_100_ms"].as_u64().unwrap() <= 1;
    let stages: BTreeMap<_, _> = samples.stages.into_iter().map(|(stage, samples)| {
        let values: Vec<_> = samples.into_iter().filter(|(time, _)| *time >= start && *time < end).map(|(_, elapsed)| elapsed).collect();
        (stage, json!({"samples": values.len(), "median_ms": percentile(&values, 0.5), "p95_ms": percentile(&values, 0.95)}))
    }).collect();
    let scanouts: BTreeMap<_, _> = samples.scanouts.into_iter().filter_map(|(output, events)| {
        let mut times = BTreeMap::new();
        for (presented, observed, hardware_clock) in events {
            if observed >= start && observed < end {
                times.entry((presented * 1_000_000.0).round() as u64).or_insert(hardware_clock);
            }
        }
        if times.is_empty() { return None; }
        let intervals: Vec<_> = times.keys().copied().collect::<Vec<_>>().windows(2)
            .map(|pair| (pair[1] - pair[0]) as f64 / 1000.0).collect();
        Some((output, json!({"page_flips": times.len(), "present_fps": times.len() as f64 / seconds,
            "interval_p95_ms": percentile(&intervals, 0.95), "maximum_interval_ms": percentile(&intervals, 1.0),
            "hardware_timestamps": times.values().filter(|value| **value).count(),
            "dispatch_timestamps": times.values().filter(|value| !**value).count()})))
    }).collect();
    let physical_present_fps = if scanouts.len() == 1 { scanouts.values().next().map(|value| value["present_fps"].clone()) } else { None };
    Ok(
        json!({"measurement_start_unix": start, "warmup_seconds": warmup, "measurement_seconds": seconds,
        "selected_surface": selected, "windows": windows, "stages": stages, "physical_present_fps": physical_present_fps,
        "scanout_outputs": scanouts,
        "scanout_scope": "Completed DRM page flips per output; timestamps distinguish kernel monotonic time from dispatch fallback. Single-output FPS is also reported above; multiple outputs remain separate. Panel refresh, fresh client content and human-visible fidelity are separate checks.",
        "scope": "Fresh client content observed after composition; callback counts and physical scanout are separate measurements.",
        "gate": {"minimum_fps": minimum_fps, "maximum_p95_ms": 25, "maximum_stalls_over_100_ms": 1, "passed": passed}}),
    )
}

pub(super) fn analyze(arguments: &[OsString]) -> Result<()> {
    let mut arguments = arguments.iter();
    let path = arguments
        .next()
        .ok_or("analyze requires a session trace path")?;
    let mut warmup = 10.0;
    let mut seconds = 60.0;
    let mut minimum_fps = 57.0;
    let mut surface = None;
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or("Measurement option requires a value")?
            .to_str()
            .ok_or("Invalid measurement value")?;
        match argument.to_str() {
            Some("--warmup") => warmup = value.parse().map_err(error)?,
            Some("--seconds") => seconds = value.parse().map_err(error)?,
            Some("--minimum-fps") => minimum_fps = value.parse().map_err(error)?,
            Some("--surface") => surface = Some(value.parse().map_err(error)?),
            _ => return Err(
                "Use analyze LOG [--warmup 10] [--seconds 60] [--minimum-fps 57] [--surface ID]"
                    .into(),
            ),
        }
    }
    if !(0.0..=600.0).contains(&warmup)
        || !(1.0..=3600.0).contains(&seconds)
        || !(1.0..=1000.0).contains(&minimum_fps)
    {
        return Err("Invalid measurement duration or frame-rate threshold".into());
    }
    let report = summarize(
        parse(&fs::read_to_string(Path::new(path)).map_err(error)?),
        warmup,
        seconds,
        minimum_fps,
        surface,
    )?;
    println!("{}", serde_json::to_string_pretty(&report).map_err(error)?);
    if report["gate"]["passed"] == true {
        Ok(())
    } else {
        Err("Presentation performance gate failed; see the JSON report".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_callbacks_and_duplicate_outputs_do_not_count_as_new_content() {
        let log = (0..=120)
            .map(|frame| {
                format!(
                    "Park app-presentation: 1 7 1 {:.6}\n",
                    1000.0 + frame as f64 / 60.0
                )
            })
            .collect::<String>();
        let report = summarize(parse(&log), 0.0, 2.0, 57.0, None).unwrap();
        assert_eq!(report["windows"]["1"]["fresh_frames"], 1);
        assert_eq!(report["gate"]["passed"], false);
    }

    #[test]
    fn showoff_stress_miss_is_not_rounded_into_a_pass() {
        let log = (0..=134)
            .map(|frame| {
                format!(
                    "Park app-presentation: 1 7 {frame} {:.6}\n",
                    1000.0 + frame as f64 / 44.5
                )
            })
            .collect::<String>();
        let report = summarize(parse(&log), 1.0, 2.0, 45.0, None).unwrap();
        assert!(report["windows"]["1"]["fresh_fps"].as_f64().unwrap() < 45.0);
        assert_eq!(report["gate"]["passed"], false);
    }

    #[test]
    fn warmup_grouping_and_stage_times_share_the_measured_window() {
        let mut log = String::new();
        for frame in 0..=180 {
            let time = 1000.0 + frame as f64 / 60.0;
            for node in [7, 8] {
                for _ in 0..2 {
                    log.push_str(&format!(
                        "Park app-presentation: 1 {node} {frame} {time:.6}\n"
                    ));
                }
            }
            log.push_str(&format!("Park frame native-update: 4.0 ms at {time:.6}\n"));
        }
        let report = summarize(parse(&log), 1.0, 2.0, 57.0, Some(1)).unwrap();
        assert_eq!(report["windows"]["1"]["fresh_frames"], 120);
        assert_eq!(report["gate"]["passed"], true);
        assert_eq!(report["stages"]["native-update"]["p95_ms"], 4.0);
        assert!(report["physical_present_fps"].is_null());
        assert!(summarize(parse(&log), 1.0, 60.0, 57.0, None).is_err());
    }

    #[test]
    fn scanout_counts_are_per_output_and_use_kernel_intervals_despite_dispatch_jitter() {
        let mut log = String::new();
        for frame in 0..=120 {
            let time = 1000.0 + frame as f64 / 60.0;
            let presented = 42.0 + frame as f64 / 60.0;
            let observed = time + if frame % 2 == 0 { 0.001 } else { 0.012 };
            log.push_str(&format!("Park app-presentation: 1 7 {frame} {time:.6}\n"));
            for _ in 0..2 {
                log.push_str(&format!("Park scanout: 1 {presented:.9} {observed:.6} drm\n"));
            }
        }
        let report = summarize(parse(&log), 0.0, 2.0, 57.0, None).unwrap();
        assert_eq!(report["physical_present_fps"], 60.0);
        assert_eq!(report["scanout_outputs"]["1"]["hardware_timestamps"], 120);
        assert_eq!(report["scanout_outputs"]["1"]["dispatch_timestamps"], 0);
        assert!(report["scanout_outputs"]["1"]["interval_p95_ms"].as_f64().unwrap() < 17.0);
        log.push_str(&log.lines().filter_map(|line| line.strip_prefix("Park scanout: 1 "))
            .map(|line| format!("Park scanout: 2 {line}\n")).collect::<String>());
        let report = summarize(parse(&log), 0.0, 2.0, 57.0, None).unwrap();
        assert!(report["physical_present_fps"].is_null());
        assert_eq!(report["scanout_outputs"]["1"]["present_fps"], 60.0);
        assert_eq!(report["scanout_outputs"]["2"]["present_fps"], 60.0);
    }

    #[test]
    fn late_active_applications_define_their_own_measurement_window_after_startup_probes() {
        let mut log = String::new();
        for frame in 0..=7200 {
            let time = 1000.0 + frame as f64 / 60.0;
            log.push_str(&format!("Park app-presentation: 1 7 1 {time:.6}\n"));
            if frame >= 3000 {
                log.push_str(&format!("Park app-presentation: 2 8 {frame} {time:.6}\n"));
            }
        }
        let report = summarize(parse(&log), 10.0, 60.0, 57.0, None).unwrap();
        assert_eq!(report["selected_surface"], 2);
        assert_eq!(report["measurement_start_unix"], 1060.0);
        assert_eq!(report["windows"]["2"]["fresh_fps"], 60.0);
        assert_eq!(report["gate"]["passed"], true);
        assert!(report["physical_present_fps"].is_null());
    }
}
