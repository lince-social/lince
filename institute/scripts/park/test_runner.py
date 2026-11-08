import importlib.util
import json
import struct
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from test_report import window

spec = importlib.util.spec_from_file_location("park_runner", Path(__file__).with_name("performance.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def manifest(self, directory):
        artifact = directory / "precompiled-test"
        artifact.write_bytes(b"private fixture, never executed")
        return {"schema_version": 1, "revision": "fixture-revision", "comparison_identity": {"workload": "private-unit-fixture"},
                "artifacts": {name: {"path": artifact.name, "sha256": runner.digest(artifact)}
                              for name in ("interface-tests", "park-tests")},
                "cases": runner.suite_cases()}

    def test_entire_catalogue_has_valid_distinct_cases_and_explicit_phase_contracts(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest = self.manifest(directory)
            (directory / "manifest.json").write_text(json.dumps(manifest))
            self.assertEqual(runner.check_bundle(directory), manifest)
        edits = [case for case in manifest["cases"] if case.get("edit_probe")]
        self.assertEqual(len(edits), 6)
        self.assertEqual({case["environment"]["LINCE_EDIT_PROBE_VIEWS"] for case in edits}, {"1", "8"})
        self.assertTrue(all(case["phases"] == ["warmup", "quiet", "local-save", "reopen"] for case in edits))

    def test_missing_artifact_fails_without_invoking_a_build_or_child_process(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest = self.manifest(directory)
            (directory / "precompiled-test").unlink()
            (directory / "manifest.json").write_text(json.dumps(manifest))
            with patch.object(runner.subprocess, "run") as run, patch.object(runner.subprocess, "Popen") as spawn:
                with self.assertRaisesRegex(ValueError, "Missing or escaping"):
                    runner.check_bundle(directory)
                run.assert_not_called()
                spawn.assert_not_called()

    def test_changed_artifact_and_escaping_path_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest = self.manifest(directory)
            (directory / "precompiled-test").write_bytes(b"different source")
            (directory / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, "checksum"):
                runner.check_bundle(directory)
            with self.assertRaisesRegex(ValueError, "escaping"):
                runner.artifact_path(directory, "../manifest.json")

    def test_missing_or_duplicate_expected_phase_is_a_manifest_error(self):
        for phases in (None, [], ["quiet", "quiet"], ["bad/value"], [{}]):
            with self.subTest(phases=phases), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                manifest = self.manifest(directory)
                manifest["cases"][1]["phases"] = phases
                (directory / "manifest.json").write_text(json.dumps(manifest))
                with self.assertRaisesRegex(ValueError, "expected phases"):
                    runner.check_bundle(directory)

    def test_required_phase_or_measurement_missing_is_never_a_zero_cost_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "performance.jsonl"
            path.write_text("Lince performance " + json.dumps(window()) + "\n")
            case = {"phases": ["typing", "reopen"], "required_signals": {
                "typing": {"stages": ["native-main", "missing-owner"], "positive_counters": ["missing-work"]}}}
            values = runner.phase_observations(path, case)
            self.assertFalse(values["typing"]["complete"])
            self.assertIn("required-stage-unobserved:missing-owner", values["typing"]["issues"])
            self.assertIn("required-work-unobserved:missing-work", values["typing"]["issues"])
            self.assertFalse(values["reopen"]["complete"])

    def test_valid_measured_work_is_accepted_only_in_its_declared_phase(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "performance.jsonl"
            report = window()
            report["counters"] = [{"counter": "real-work", "count": 3}]
            path.write_text("Lince performance " + json.dumps(report) + "\n")
            case = {"phases": ["typing"], "required_signals": {
                "typing": {"stages": ["native-main"], "positive_counters": ["real-work"]}}}
            self.assertTrue(runner.phase_observations(path, case)["typing"]["complete"])

    def test_passing_test_processes_cannot_hide_incomplete_measurements(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest = self.manifest(directory)
            (directory / "manifest.json").write_text(json.dumps(manifest))
            arguments = SimpleNamespace(bundle=directory, runtime=None, output=directory / "evidence",
                                        case=["combined-native"], capture="on")

            def execute(root, manifest, case, output, prefix, capture):
                result = {"case": case["id"], "passed": True}
                if case.get("performance"):
                    result.update(performance={}, phases={"typing": {"complete": False, "issues": ["final-report-missing"]}})
                return result

            with patch.object(runner, "runtime_prefix", return_value=[]), patch.object(runner, "execute_case", side_effect=execute), patch("builtins.print"):
                with self.assertRaisesRegex(ValueError, "observations incomplete"):
                    runner.run_suite(arguments)
            evidence = json.loads((arguments.output / "run.json").read_text())
            self.assertTrue(evidence["selected_checks_passed"])
            self.assertFalse(evidence["performance_complete"])
            self.assertFalse(evidence["physical_acceptance"])

    def test_memory_filesystem_cannot_masquerade_as_durable_storage_timing(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            manifest = self.manifest(directory)
            (directory / "manifest.json").write_text(json.dumps(manifest))
            arguments = SimpleNamespace(bundle=directory, runtime=None, output=directory / "evidence",
                                        case=["local-edit-title-one-view"], capture="on")
            with patch.object(runner, "runtime_prefix", return_value=[]), patch.object(runner, "filesystem_identity", return_value={"type": "tmpfs"}), patch.object(runner, "execute_case") as execute:
                with self.assertRaisesRegex(ValueError, "disk-backed"):
                    runner.run_suite(arguments)
                execute.assert_not_called()

    def test_installed_nix_wrapper_finds_sibling_elf_without_executing_shell(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            wrapper = directory / "lince"
            wrapper.write_text("#!/bin/sh\nexit 99\n")
            elf = directory / ".lince-wrapped"
            elf.write_bytes(b"\x7fELFprivate fixture")
            with patch.object(runner.shutil, "which", return_value=str(wrapper)), patch.object(runner.subprocess, "run") as run:
                self.assertEqual(runner.installed_runtime(), elf)
                run.assert_not_called()

    def test_runtime_in_niri_uses_installed_elf_and_expands_origin_for_test_loader(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            elf, loader = directory / ".lince-wrapped", directory / "loader"
            elf.touch()
            loader.touch()
            with patch.object(runner, "active_runtime", return_value=None), patch.object(runner, "installed_runtime", return_value=elf), patch.object(
                runner, "elf_runtime", return_value=(str(loader), "$ORIGIN/lib:${ORIGIN}/deps")
            ), patch.object(runner.subprocess, "run") as run:
                self.assertEqual(runner.runtime_prefix(None), [str(loader), "--library-path", f"{directory}/lib:{directory}/deps"])
                run.assert_not_called()

    def test_elf_metadata_reads_interpreter_and_runpath_without_sections_or_external_tools(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "runtime"
            interpreter = b"/private/loader\0"
            strings = b"\0$ORIGIN/lib:/private/libs\0"
            header = struct.pack("<16sHHIQQQIHHHHHH", b"\x7fELF\x02\x01" + bytes(10), 3, 62, 1, 0, 64, 0, 0, 64, 56, 3, 0, 0, 0)
            dynamic = b"".join(struct.pack("<QQ", *entry) for entry in ((5, 0x1000 + 512), (10, len(strings)), (29, 1), (0, 0)))
            segments = b"".join(struct.pack("<IIQQQQQQ", *entry) for entry in (
                (1, 4, 0, 0x1000, 0, 1024, 1024, 4096),
                (3, 4, 256, 0, 0, len(interpreter), len(interpreter), 1),
                (2, 4, 320, 0, 0, len(dynamic), len(dynamic), 8),
            ))
            value = bytearray(1024)
            value[:len(header + segments)] = header + segments
            value[256:256 + len(interpreter)] = interpreter
            value[320:320 + len(dynamic)] = dynamic
            value[512:512 + len(strings)] = strings
            path.write_bytes(value)
            self.assertEqual(runner.elf_runtime(path), ("/private/loader", "$ORIGIN/lib:/private/libs"))
            value[320 + 2 * 16:320 + 3 * 16] = struct.pack("<QQ", 15, 1)
            path.write_bytes(value)
            self.assertEqual(runner.elf_runtime(path), ("/private/loader", "$ORIGIN/lib:/private/libs"))
            path.write_bytes(value[:128])
            with self.assertRaisesRegex(ValueError, "Truncated"):
                runner.elf_runtime(path)
            path.write_bytes(bytes(64))
            with self.assertRaisesRegex(ValueError, "ELF64"):
                runner.elf_runtime(path)



if __name__ == "__main__":
    unittest.main()
