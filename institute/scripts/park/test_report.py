import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

from report import Distribution, changes_manifest, compare, summarize


def reading(total=20, count=2):
    return {"stage": "native-main", "kind": "inclusive-elapsed", "count": count, "total_us": total,
            "maximum_us": total // count, "buckets": [count, 0], "p50_upper_us": 10, "p95_upper_us": 10, "p99_upper_us": 10}


def window(sequence=1, begin=0, end=1000, final=True, phase="typing"):
    return {"schema_version": 2, "histogram_limits_us": [10, 100], "process": 1, "sequence": sequence,
            "window_started_us": begin, "window_ended_us": end, "final_report": final, "phase": phase,
            "dropped_reports": 0, "stages": [reading()], "outputs": [], "counters": [], "coverage": []}


def changes():
    return {"schema_version": 1, "changes": [{"id": "layout-root", "task": "P122", "owner": "layout-collection",
            "mechanism": "Collect only affected roots", "expected": "Fewer visited roots with unchanged pixels",
            "primary_metrics": ["stage/native-main"], "secondary_metrics": [], "paths": ["native/layout.rs"]}]}


class ReportTests(unittest.TestCase):
    def summary(self, windows, phase=None):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trace.log"
            path.write_text("".join("Lince performance " + json.dumps(value) + "\n" for value in windows))
            return summarize(path, phase)

    def test_histograms_merge_observations_instead_of_averaging_percentiles(self):
        first = reading(100, 10)
        second = reading(100, 1)
        second["buckets"] = [0, 1]
        value = Distribution()
        value.add(first, [10, 100])
        value.add(second, [10, 100])
        result = value.result(1000)
        self.assertEqual(result["p50_upper_us"], 10)
        self.assertEqual(result["p95_upper_us"], 100)
        self.assertEqual(result["count"], 11)

    def test_gaps_truncation_and_missing_finalization_are_not_a_pass(self):
        report = self.summary([window(final=False), window(sequence=3, begin=1000, end=2000)])
        self.assertFalse(report["complete"])
        self.assertIn("report-sequence-or-window-gap", report["issues"])
        report = self.summary([window(final=False)])
        self.assertIn("final-report-missing", report["issues"])

    def test_phase_comparison_preserves_complete_global_capture_identity(self):
        report = self.summary([window(final=False, phase="warmup"), window(sequence=2, begin=1000, end=2000)], "typing")
        self.assertTrue(report["complete"])
        self.assertEqual(report["reports"], 1)
        self.assertEqual(report["observation_us"], 1000)
        self.assertEqual(report["stages"]["stage/native-main"]["count"], 2)

    def test_empty_signal_is_unobserved_and_negative_values_fail(self):
        value = Distribution()
        self.assertEqual(value.result(1000)["status"], "unobserved")
        invalid = reading()
        invalid["total_us"] = -1
        with self.assertRaises(ValueError):
            value.add(invalid, [10, 100])

    def test_competing_owner_changes_are_rejected_before_batch_delivery(self):
        value = changes()
        value["changes"].append({**value["changes"][0], "id": "second-attempt"})
        with self.assertRaises(ValueError):
            changes_manifest(value)

    def test_different_owner_names_cannot_hide_the_same_primary_cost(self):
        value = changes()
        value["changes"].append({**value["changes"][0], "id": "another-attempt", "owner": "another-name"})
        with self.assertRaises(ValueError):
            changes_manifest(value)

    def test_neutral_total_preserves_opposite_phase_changes(self):
        baseline = self.summary([window()])
        baseline["stages"]["stage/gpu-wait"] = copy.deepcopy(baseline["stages"]["stage/native-main"])
        candidate = copy.deepcopy(baseline)
        candidate["stages"]["stage/native-main"]["mean_us"] = 5
        candidate["stages"]["stage/gpu-wait"]["mean_us"] = 15
        before = {"comparison_identity": {"workload": "same"}, "phases": {"typing": baseline}}
        after = {"comparison_identity": {"workload": "same"}, "phases": {"typing": candidate}}
        result = compare(before, after, changes())
        deltas = {row["metric"]: row["values"]["mean_us"]["delta"] for row in result["rows"]}
        self.assertEqual(deltas, {"stage/gpu-wait": 5, "stage/native-main": -5})
        self.assertEqual(next(row for row in result["rows"] if row["metric"] == "stage/native-main")["primary_changes"], ["layout-root"])

    def test_profile_or_workload_change_cannot_produce_a_speedup_claim(self):
        phase = self.summary([window()])
        before = {"comparison_identity": {"workload": "normal"}, "phases": {"typing": phase}}
        after = {"comparison_identity": {"workload": "empty"}, "phases": {"typing": phase}}
        with self.assertRaisesRegex(ValueError, "identity differs"):
            compare(before, after, changes())

    def test_zero_measured_phases_cannot_be_a_successful_comparison(self):
        empty = {"comparison_identity": "same", "phases": {}}
        with self.assertRaisesRegex(ValueError, "No measured phases"):
            compare(empty, empty, changes())

    def test_missing_baseline_signal_is_not_invented_as_zero(self):
        baseline = self.summary([window()])
        candidate = copy.deepcopy(baseline)
        candidate["stages"]["stage/new-only"] = reading()
        result = compare({"comparison_identity": "same", "phases": {"typing": baseline}},
                         {"comparison_identity": "same", "phases": {"typing": candidate}}, changes())
        self.assertFalse(result["complete"])
        self.assertTrue(any(row.get("metric") == "stage/new-only" for row in result["missing"]))

    def test_selected_subset_always_runs_protein_first_and_rejects_unknown(self):
        path = Path(__file__).with_name("performance.py")
        spec = importlib.util.spec_from_file_location("park_performance", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        manifest = {"cases": [{"id": "protein-first-interface"}, {"id": "gpu-passes"}]}
        self.assertEqual([case["id"] for case in module.selected_cases(manifest, ["gpu-passes"])], ["protein-first-interface", "gpu-passes"])
        with self.assertRaises(ValueError):
            module.selected_cases(manifest, ["misspelled"])


if __name__ == "__main__":
    unittest.main()
