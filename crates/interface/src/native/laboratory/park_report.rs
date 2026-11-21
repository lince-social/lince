// non-lean-yet
// Scope: const SCHEMA, const MAX_BYTES, struct ParkReport, fn verdict.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   98, 115, 116, 117, 118, 124, 242, 247, 262.

use serde_json::Value;
use std::{fs::File, io::Read, path::Path};

const SCHEMA: &str = "park-responsiveness-v1";
const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(super) struct ParkReport {
    pub(super) source: Value,
    pub(super) lines: Vec<String>,
}

impl ParkReport {
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Err("Select a regular Park report file".into());
        }
        let file = File::open(path).map_err(|error| error.to_string())?;
        if !file
            .metadata()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("Select a regular Park report file".into());
        }
        let mut content = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|error| error.to_string())?;
        if content.len() as u64 > MAX_BYTES {
            return Err("Park report exceeds 16 MiB".into());
        }
        let source = serde_json::from_slice(&content).ok().or_else(|| {
            content.rsplit(|byte| *byte == b'\n').find_map(|line| {
                let candidate: Value = serde_json::from_slice(line).ok()?;
                (candidate["suite"] == SCHEMA).then_some(candidate)
            })
        });
        Self::from_value(source.ok_or("No final Park report found")?)
    }

    fn from_value(source: Value) -> Result<Self, String> {
        if source["schema_version"] == 1 && source["workflow_coverage"]["schema_version"] == 1 {
            return Self::workflow_coverage(source);
        }
        if source["suite"] != SCHEMA {
            return Err("Select a Park responsiveness report".into());
        }
        let revision = source["revision"].as_str().unwrap_or_default();
        let identified =
            revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit());
        let optimized = source["optimized"].as_bool();
        let delivered = source["measurements_delivered"] == true;
        let coverage = source["coverage"]["passed"] == true;
        let cases = source["cases"]
            .as_array()
            .ok_or("Park report has no cases")?;
        let labels = [
            ("protein-first-interface", "Interface checks"),
            ("empty", "Empty workspace"),
            ("empty-with-inactive-content", "Inactive content"),
            (
                "mixed-content-under-query-pressure",
                "Mixed content and queries",
            ),
            ("worker-idle-and-input", "Idle canvas worker and input"),
            (
                "native-pixels-resize-and-output-mapping",
                "Pixels and output mapping",
            ),
        ];
        let mut results = Vec::new();
        let mut complete = cases.len() == labels.len();
        let mut all_passed = complete;
        for (id, label) in labels {
            let mut matches = cases.iter().filter(|case| case["case"] == id);
            let row = matches.next().filter(|_| matches.next().is_none());
            let passed = row.and_then(|row| row["passed"].as_bool());
            complete &= passed.is_some();
            all_passed &= passed == Some(true);
            results.push(format!("{label} · {}", verdict(passed)));
            if let Some(row) = row {
                if let Some(checks) = row["results"].as_array() {
                    let passed = checks
                        .iter()
                        .filter(|check| check["error"].is_null())
                        .count();
                    results.push(format!("  {passed}/{} checks passed", checks.len()));
                    if let Some(failed) = checks.iter().find(|check| !check["error"].is_null())
                        && let Some(name) = failed["name"].as_str()
                    {
                        results.push(format!(
                            "  First failure · {}",
                            name.chars().take(160).collect::<String>()
                        ));
                    }
                }
                for field in ["error", "typing_error"] {
                    if let Some(error) = row[field].as_str() {
                        let detail: String = error.chars().take(240).collect();
                        results.push(format!(
                            "  {}",
                            detail.split_whitespace().collect::<Vec<_>>().join(" ")
                        ));
                    }
                }
                for (field, action) in [
                    ("edit", "Edit"),
                    ("typing", "Typing"),
                    ("scrolling", "Scrolling"),
                    ("dragging", "Dragging"),
                    ("motion", "Pan to GPU completion"),
                    ("input_to_completed_publication", "Input to publication"),
                ] {
                    if let (Some(p95), Some(p99), Some(worst), Some(samples)) = (
                        row[field]["p95_ms"].as_f64(),
                        row[field]["p99_ms"].as_f64(),
                        row[field]["worst_ms"].as_f64(),
                        row[field]["count"].as_u64(),
                    ) {
                        results.push(format!(
                            "  {action} · {samples} samples · p95 {p95:.2}/33.4 ms · p99 {p99:.2}/50 ms · worst {worst:.2}/100 ms"
                        ));
                        if field == "motion"
                            && let Some(values) = row[field]["samples_ms"].as_array()
                            && let Ok(total) = usize::try_from(samples)
                            && total == values.len()
                            && total > 0
                            && values
                                .iter()
                                .all(|value| value.as_f64().is_some_and(|ms| ms >= 0.0))
                        {
                            let missed = values
                                .iter()
                                .filter(|value| value.as_f64().is_some_and(|ms| ms > 1000.0 / 60.0))
                                .count();
                            results
                                .push(format!("  Above 16.67 ms · {missed}/{total} · budget 1%"));
                        }
                    }
                }
                if let Some(cpu) = row["idle_cpu_percent_one_core"].as_f64() {
                    results.push(format!("  Idle CPU {cpu:.2}% of one core · budget 2%"));
                }
            }
        }
        let status = if !complete || !identified || !delivered || !coverage || optimized.is_none() {
            "Incomplete"
        } else if source["passed"] == true && all_passed && optimized == Some(true) {
            "Passed"
        } else {
            "Failed"
        };
        let build = match optimized {
            Some(true) => "optimized",
            Some(false) => "debug",
            None => "profile unavailable",
        };
        let mut lines = vec![format!("Park responsiveness · {status} · {build}")];
        if identified {
            lines.push(format!("Measured revision {revision}"));
            if utils::build_info::revision_is_stamped() && utils::build_info::revision() != revision
            {
                lines.push(format!(
                    "Laboratory is running revision {}",
                    utils::build_info::short_revision()
                ));
            }
        }
        lines.push(
            "Scope: isolated native rendering and input · physical presentation outside this run"
                .into(),
        );
        lines.push(format!(
            "Trace delivery {} · measurement coverage {}",
            verdict(source["measurements_delivered"].as_bool()),
            verdict(source["coverage"]["passed"].as_bool())
        ));
        if let Some(gpu) = source["coverage"]["hardware_gpu_execution"].as_bool() {
            lines.push(
                if gpu {
                    "Hardware GPU timing recorded"
                } else {
                    "Hardware GPU timing unavailable"
                }
                .into(),
            );
        }
        if let Some(loss) = source["coverage"]["loss"].as_object() {
            for (field, label) in [
                ("dropped_reports", "Timing windows"),
                ("dropped_system_samples", "System samples"),
                ("dropped_span_trace", "Timing spans"),
                ("dropped_input_trace", "Input records"),
                ("dropped_park_frames", "Frame records"),
            ] {
                if let Some(count) = loss.get(field) {
                    match count.as_u64() {
                        Some(0) => {}
                        Some(count) => lines.push(format!("{label} lost · {count}")),
                        None => lines.push(format!("{label} · loss count unavailable")),
                    }
                }
            }
        }
        lines.extend(results);
        Ok(Self { source, lines })
    }

    fn workflow_coverage(source: Value) -> Result<Self, String> {
        let coverage = &source["workflow_coverage"];
        let pairs = coverage["pairs"]
            .as_array()
            .filter(|pairs| !pairs.is_empty())
            .ok_or("Park workflow report has no factor pairs")?;
        let required = coverage["required"]
            .as_array()
            .filter(|rows| !rows.is_empty())
            .ok_or("Park workflow report has no required combinations")?;
        let selected = coverage["selected_cases"]
            .as_array()
            .filter(|cases| {
                cases
                    .first()
                    .is_some_and(|case| case == "protein-first-interface")
            })
            .ok_or("Park workflow report is missing its mandatory first case")?;
        let states = [
            "passed",
            "failed",
            "not_run",
            "selector_unavailable",
            "missing_oracle",
            "inapplicable",
        ];
        let mut counts = [0; 6];
        for pair in pairs {
            let status = pair["status"].as_str().unwrap_or_default();
            let index = states
                .iter()
                .position(|state| *state == status)
                .ok_or("Park workflow report has an unknown pair outcome")?;
            counts[index] += 1;
        }
        for requirement in required {
            if !requirement["status"]
                .as_str()
                .is_some_and(|status| states[..5].contains(&status))
            {
                return Err("Park workflow report has an unknown required outcome".into());
            }
        }
        let required_passed = required
            .iter()
            .filter(|requirement| requirement["status"] == "passed")
            .count();
        let mut passed_cases = 0;
        let mut failed_cases = 0;
        for case in selected {
            let case = case
                .as_str()
                .ok_or("Park workflow report has an invalid selected case")?;
            match coverage["case_results"][case].as_str() {
                Some("passed") => passed_cases += 1,
                Some("failed") => failed_cases += 1,
                None => {}
                _ => return Err("Park workflow report has an unknown case outcome".into()),
            }
        }
        let [passed, failed, unrun, unavailable, missing, excluded] = counts;
        let applicable = pairs.len() - excluded;
        let status = if failed_cases > 0
            || failed > 0
            || required
                .iter()
                .any(|requirement| requirement["status"] == "failed")
        {
            "Failed"
        } else if passed == applicable
            && required_passed == required.len()
            && passed_cases == selected.len()
            && coverage["complete"] == true
        {
            "Covered"
        } else {
            "Incomplete"
        };
        let mut lines = vec![
            format!("Park workflow coverage · {status}"),
            format!("{passed}/{applicable} applicable pairs passed · {excluded} inapplicable"),
            format!(
                "{failed} failed · {unrun} unrun · {unavailable} unavailable · {missing} missing oracles"
            ),
            format!(
                "{required_passed}/{} required combinations passed",
                required.len()
            ),
            format!(
                "{passed_cases}/{} selected private cases passed",
                selected.len()
            ),
            "Physical daily-workload acceptance is separate".into(),
        ];
        if let Some(revision) = source["revision"].as_str().filter(|revision| {
            revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            lines.push(format!("Prepared revision {revision}"));
        }
        for requirement in required
            .iter()
            .filter(|requirement| requirement["status"] != "passed")
            .take(6)
        {
            let id = requirement["id"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(16)
                .collect::<String>();
            let status = requirement["status"]
                .as_str()
                .unwrap_or_default()
                .replace('_', " ");
            lines.push(format!("{id} · {status}"));
        }
        Ok(Self { source, lines })
    }
}

fn verdict(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "passed",
        Some(false) => "failed",
        None => "unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn workflow_import_uses_case_and_pair_outcomes_before_the_supplied_total() {
        let mut source = json!({
            "schema_version": 1,
            "workflow_coverage": {
                "schema_version": 1,
                "complete": true,
                "pairs": [{"status":"missing_oracle"}, {"status":"inapplicable"}],
                "required": [{"id":"R08", "status":"not_run"}],
                "selected_cases": ["protein-first-interface"],
                "case_results": {"protein-first-interface": "passed"}
            }
        });
        let report = ParkReport::from_value(source.clone()).unwrap();
        assert!(report.lines[0].ends_with("Incomplete"));
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.contains("1 missing oracles"))
        );
        source["workflow_coverage"]["case_results"]["protein-first-interface"] = json!("failed");
        assert!(ParkReport::from_value(source).unwrap().lines[0].ends_with("Failed"));
    }
}
