import collections
import json
import math
from pathlib import Path

PREFIX = "Lince performance "
MAX_LOG_BYTES = 64 * 1024 * 1024


def load_json(path):
    with Path(path).open(encoding="utf-8") as source:
        return json.load(source)


def save_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("w", encoding="utf-8") as target:
        json.dump(value, target, indent=2, allow_nan=False)
        target.write("\n")
    temporary.replace(path)


def nonnegative(value, label):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
        raise ValueError(f"Invalid nonnegative {label}")
    return value


class Distribution:
    def __init__(self):
        self.count = 0
        self.total = 0
        self.maximum = 0
        self.kind = None
        self.limits = None
        self.buckets = None

    def add(self, reading, limits):
        count = nonnegative(reading["count"], "count")
        total = nonnegative(reading["total_us"], "total")
        maximum = nonnegative(reading["maximum_us"], "maximum")
        kind = reading["kind"]
        if self.kind is not None and self.kind != kind:
            raise ValueError("Timing kind changed inside a run")
        if not count:
            if total or maximum:
                raise ValueError("An empty distribution contains time")
            return
        if not isinstance(count, int) or total > count * maximum:
            raise ValueError("Invalid distribution accounting")
        if not limits or any(a >= b for a, b in zip(limits, limits[1:])):
            raise ValueError("Invalid histogram limits")
        buckets = reading["buckets"]
        if len(buckets) != len(limits) or sum(buckets) != count:
            raise ValueError("Histogram count mismatch")
        for bucket in buckets:
            if not isinstance(bucket, int) or bucket < 0:
                raise ValueError("Invalid histogram bucket")
        if self.limits is not None and self.limits != limits:
            raise ValueError("Histogram boundaries changed inside a run")
        self.kind = kind
        self.limits = list(limits)
        self.buckets = [a + b for a, b in zip(self.buckets or [0] * len(limits), buckets)]
        self.count += count
        self.total += total
        self.maximum = max(self.maximum, maximum)

    def result(self, duration_us):
        result = {"count": self.count, "kind": self.kind, "total_us": self.total, "maximum_us": self.maximum}
        if not self.count:
            return {**result, "status": "unobserved"}
        result.update(status="observed", mean_us=self.total / self.count)
        result["tail_sample_warning"] = "short-cohort; p99-is-an-upper-bound-with-weak-tail-evidence" if self.count < 1000 else None
        result["total_us_per_observation_second"] = self.total * 1_000_000 / duration_us if duration_us else None
        for quantile in (50, 95, 99):
            needed = (self.count * quantile + 99) // 100
            cumulative = 0
            for limit, count in zip(self.limits, self.buckets):
                cumulative += count
                if cumulative >= needed:
                    result[f"p{quantile}_upper_us"] = min(limit, self.maximum)
                    break
        return result


def summarize(path, phase=None):
    path = Path(path)
    if path.stat().st_size > MAX_LOG_BYTES:
        raise ValueError("Performance log exceeds the bounded report size")
    stages = collections.defaultdict(Distribution)
    counters = collections.Counter()
    counter_catalog = set()
    outputs = {}
    render = {}
    coverage = {}
    system_threads = {}
    unattributed_thread_samples = 0
    reports = 0
    issues = []
    windows = {}
    durations = collections.Counter()
    finalized = set()
    dropped_reports = {}
    with path.open(encoding="utf-8", errors="replace") as source:
        for line in source:
            if PREFIX not in line:
                continue
            try:
                report = json.loads(line.split(PREFIX, 1)[1])
            except json.JSONDecodeError:
                issues.append("truncated-or-invalid-report")
                continue
            if report.get("schema_version") != 2:
                issues.append("unsupported-report-schema")
                continue
            process = report["process"]
            sequence = report["sequence"]
            begin = nonnegative(report["window_started_us"], "window start")
            end = nonnegative(report["window_ended_us"], "window end")
            if end < begin:
                raise ValueError("Reversed observation window")
            previous = windows.get(process)
            if previous is None:
                if sequence != 1 or begin != 0:
                    issues.append("capture-start-missing")
            elif sequence != previous[0] + 1 or begin != previous[1]:
                issues.append("report-sequence-or-window-gap")
            if process in finalized:
                issues.append("report-after-finalization")
            windows[process] = (sequence, end)
            dropped_reports[process] = max(dropped_reports.get(process, 0), report["dropped_reports"])
            if report["final_report"]:
                finalized.add(process)
            if phase is not None and report.get("phase") != phase:
                continue
            durations[process] += end - begin
            for field in ("dropped_input_trace", "dropped_span_trace", "dropped_system_samples"):
                if report.get(field, 0):
                    issues.append(field)
            counter_catalog.update(report.get("counter_catalog", []))
            for counter in report["counters"]:
                counters[counter["counter"]] += nonnegative(counter["count"], "counter")
            for reading in report["stages"]:
                stages["stage/" + reading["stage"]].add(reading, report["histogram_limits_us"])
            for system in report.get("systems", []):
                for metric in ("wall", "owner_cpu", "exclusive_owner_cpu"):
                    stages[f"system/{system['system']}/{metric}"].add(system[metric], report["histogram_limits_us"])
                unattributed_thread_samples += nonnegative(system.get("unattributed_thread_samples", 0), "unattributed thread samples")
                for thread in system.get("executing_threads", []):
                    key = (process, system["system"], thread["thread"])
                    value = system_threads.setdefault(key, {"samples": 0, "owner_cpu_us": 0, "exclusive_owner_cpu_us": 0})
                    for field in value:
                        value[field] += nonnegative(thread[field], "system thread " + field)
            for output in report["outputs"]:
                key = (process, output["output"], output["clock"])
                value = outputs.setdefault(key, {"interval": Distribution(), "presentations": 0})
                value["presentations"] += nonnegative(output["presentations"], "presentations")
                value["interval"].add(output["interval"], report["histogram_limits_us"])
                for reading in output["stages"]:
                    stages[f"output/{output['output']}/{output['clock']}/{reading['stage']}"].add(reading, report["histogram_limits_us"])
            for reading in report.get("render_diagnostics", []):
                value = render.setdefault((reading["path"], reading["unit"]), {"count": 0, "total": 0, "maximum": 0})
                value["count"] += nonnegative(reading["count"], "render diagnostic count")
                value["total"] += nonnegative(reading["total"], "render diagnostic total")
                value["maximum"] = max(value["maximum"], nonnegative(reading["maximum"], "render diagnostic maximum"))
            for value in report.get("coverage", []):
                prior = coverage.get(value["signal"])
                if prior is None or value["status"].startswith("observed"):
                    coverage[value["signal"]] = value
            reports += 1
    if not reports:
        issues.append("no-supported-performance-reports")
    if set(windows) != finalized:
        issues.append("final-report-missing")
    if any(dropped_reports.values()):
        issues.append("dropped-reports")
    if counters["timing-capacity-exceeded"] or counters["timing-timestamps-rejected"]:
        issues.append("timing-capacity-or-clock-rejection")
    duration = sum(durations.values())
    detail_issues = sorted(set(issues) & {"dropped_input_trace", "dropped_span_trace"})
    aggregate_issues = sorted(set(issues) - set(detail_issues))
    return {
        "schema_version": 1,
        "scope": "reported-process-windows; inclusive-stages-are-not-additive",
        "reports": reports,
        "phase": phase,
        "observation_us": duration,
        "process_count": len(windows),
        "complete": not issues,
        "issues": sorted(set(issues)),
        "aggregates_complete": not aggregate_issues,
        "aggregate_issues": aggregate_issues,
        "detailed_traces_complete": not issues,
        "detail_issues": detail_issues,
        "stages": {key: value.result(duration) for key, value in sorted(stages.items())},
        "counters": {name: counters[name] for name in sorted(counter_catalog | counters.keys())},
        "coverage": list(coverage.values()),
        "system_threads": [{"process": process, "system": name, "thread": thread, **value}
                           for (process, name, thread), value in sorted(system_threads.items())],
        "system_thread_scope": "executing-thread-ids-local-to-each-process; not-physical-cores-or-cross-run-identities",
        "unattributed_system_thread_samples": unattributed_thread_samples,
        "outputs": [{
            "process": process, "output": output, "clock": clock,
            "presentations": value["presentations"],
            "presentations_per_observation_second": value["presentations"] * 1_000_000 / durations[process] if durations[process] else None,
            "interval": value["interval"].result(durations[process]),
            "physical": clock == "drm",
        } for (process, output, clock), value in sorted(outputs.items())],
        "render_diagnostics": [{
            "path": key[0], "unit": key[1], "count": values["count"],
            "mean": values["total"] / values["count"] if values["count"] else None, "maximum": values["maximum"],
            "source_frame": None, "scope": "last-completed-pass-aggregate-not-current-frame",
        } for key, values in sorted(render.items())],
    }


def changes_manifest(value):
    if value.get("schema_version") != 1 or not isinstance(value.get("changes"), list):
        raise ValueError("Invalid change attribution manifest")
    owners = set()
    identifiers = set()
    primary_metrics = set()
    for change in value["changes"]:
        for field in ("id", "task", "owner", "mechanism", "expected", "primary_metrics", "secondary_metrics", "paths"):
            if field not in change:
                raise ValueError(f"Change has no {field}")
        if not all(isinstance(change[field], str) and change[field] for field in ("id", "task", "owner", "mechanism", "expected")):
            raise ValueError("Change identities, mechanism and expectation must be nonempty strings")
        if not change["id"] or change["id"] in identifiers:
            raise ValueError("Missing or repeated change identity")
        if not change["owner"] or change["owner"] in owners:
            raise ValueError("Competing changes to the same measured owner")
        if not change["mechanism"] or not change["expected"] or not change["primary_metrics"] or not change["paths"]:
            raise ValueError("Change lacks an attributable mechanism or measurement")
        if not all(isinstance(change[field], list) for field in ("primary_metrics", "secondary_metrics", "paths")):
            raise ValueError("Invalid change measurement lists")
        if not all(isinstance(item, str) and item for field in ("primary_metrics", "secondary_metrics", "paths") for item in change[field]):
            raise ValueError("Change measurements and paths must be nonempty strings")
        if primary_metrics.intersection(change["primary_metrics"]):
            raise ValueError("Competing changes to the same primary measured cost")
        primary_metrics.update(change["primary_metrics"])
        owners.add(change["owner"])
        identifiers.add(change["id"])
    return value


def delta(before, after):
    return {"before": before, "after": after, "delta": after - before,
            "percent_change": (after - before) * 100 / before if before else None}


def measured_areas(phase):
    values = dict(phase["stages"])
    duration = phase.get("observation_us", 0)
    for name, count in phase.get("counters", {}).items():
        values["counter/" + name] = {"kind": "work-count", "count": count,
            "count_per_observation_second": count * 1_000_000 / duration if duration else None}
    for output in phase.get("outputs", []):
        prefix = f"output/{output['output']}/{output['clock']}"
        values[prefix + "/interval"] = output["interval"]
        values[prefix + "/delivery"] = {"kind": "presentation-count", "count": output["presentations"],
            "count_per_observation_second": output["presentations_per_observation_second"]}
    for diagnostic in phase.get("render_diagnostics", []):
        values["render/" + diagnostic["path"]] = {"kind": "render-diagnostic-" + diagnostic["unit"],
            "count": diagnostic["count"], "mean_value": diagnostic["mean"], "maximum_value": diagnostic["maximum"]}
    return values


def compare(before, after, changes):
    changes_manifest(changes)
    if not before.get("phases") or not after.get("phases"):
        raise ValueError("No measured phases to compare")
    identity_before = before.get("comparison_identity")
    identity_after = after.get("comparison_identity")
    per_case = bool(before.get("case_identities")) and bool(after.get("case_identities"))
    if per_case:
        identity_before = {key: value for key, value in identity_before.items() if key != "workload_sha256"}
        identity_after = {key: value for key, value in identity_after.items() if key != "workload_sha256"}
    if not identity_before or identity_before != identity_after:
        raise ValueError("Workload, quality, build-profile or observation-scope identity differs")
    rows = []
    missing = []
    for phase in sorted(set(before["phases"]) | set(after["phases"])):
        if per_case:
            case = phase.partition("/")[0]
            a = before["case_identities"].get(case)
            b = after["case_identities"].get(case)
            if not a or not b or a != b:
                missing.append({"phase": phase, "reason": "case-workload-identity-missing-or-different"})
                continue
        left = before["phases"].get(phase)
        right = after["phases"].get(phase)
        if left is None or right is None:
            missing.append({"phase": phase, "reason": "missing-phase"})
            continue
        previous_values, current_values = measured_areas(left), measured_areas(right)
        for metric in sorted(set(previous_values) | set(current_values)):
            previous = previous_values.get(metric)
            current = current_values.get(metric)
            if not previous or not current or previous.get("kind") != current.get("kind"):
                missing.append({"phase": phase, "metric": metric, "reason": "missing-or-incomparable-signal"})
                continue
            owners = [change for change in changes["changes"] if metric in change["primary_metrics"]]
            secondary = [change["id"] for change in changes["changes"] if metric in change["secondary_metrics"]]
            row = {"phase": phase, "metric": metric, "kind": previous["kind"], "values": {},
                   "primary_changes": [change["id"] for change in owners], "possible_secondary_changes": secondary,
                   "attribution": "single-declared-owner; verify-predicted-work-and-controls" if len(owners) == 1 and not secondary else "shared-or-unassigned; inspect-timeline"}
            for field in ("count", "total_us", "mean_us", "total_us_per_observation_second", "p50_upper_us", "p95_upper_us", "p99_upper_us", "maximum_us", "count_per_observation_second", "mean_value", "maximum_value"):
                a, b = previous.get(field), current.get(field)
                if a is not None and b is not None:
                    row["values"][field] = delta(a, b)
            rows.append(row)
    return {"schema_version": 1, "complete": bool(rows) and not missing and all(phase.get("complete") for run in (before, after) for phase in run["phases"].values()),
            "comparison_identity": identity_before, "rows": rows, "missing": missing,
            "scope": "observed-phase-deltas; no-summed-desktop-percentage-or-automatic-causation"}


def readable(comparison):
    lines = ["Park combined comparison", "Negative duration deltas mean less measured cost; inclusive rows must not be added."]
    for row in comparison["rows"]:
        values = row["values"]
        field = next((name for name in ("mean_us", "count_per_observation_second", "mean_value", "count") if name in values), None)
        mean = values.get(field)
        if mean is None:
            continue
        percent = f"{mean['percent_change']:+.2f}%" if mean["percent_change"] is not None else "undefined baseline percentage"
        lines.append(f"{row['phase']} {row['metric']}: {mean['before']:.2f} -> {mean['after']:.2f} {field} ({percent}); {row['attribution']}")
    for missing in comparison["missing"]:
        lines.append("UNAVAILABLE " + json.dumps(missing, sort_keys=True))
    return "\n".join(lines) + "\n"
