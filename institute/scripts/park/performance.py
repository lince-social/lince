import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import time

from report import MAX_LOG_BYTES, compare, load_json, readable, save_json, summarize


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as source:
        for data in iter(lambda: source.read(1024 * 1024), b""):
            value.update(data)
    return value.hexdigest()


def artifact_path(root, value):
    path = (root / value).resolve()
    if not path.is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError(f"Missing or escaping precompiled artifact: {value}")
    return path


def check_bundle(root):
    manifest = load_json(root / "manifest.json")
    if manifest.get("schema_version") != 1 or not manifest.get("revision"):
        raise ValueError("Invalid validation artifact manifest")
    for artifact in manifest["artifacts"].values():
        path = artifact_path(root, artifact["path"])
        if digest(path) != artifact["sha256"]:
            raise ValueError("Precompiled artifact checksum mismatch")
    if not manifest.get("cases") or manifest["cases"][0]["id"] != "protein-first-interface":
        raise ValueError("The combined suite must start with the common Protein-first interface gate")
    identities = set()
    for case in manifest["cases"]:
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]*", case["id"]) or case["id"] in identities:
            raise ValueError("Invalid or duplicated case identity")
        if case["artifact"] not in manifest["artifacts"] or not re.fullmatch(r"[A-Za-z0-9_:]+", case["selector"]):
            raise ValueError("Invalid precompiled selector")
        if not isinstance(case["timeout_seconds"], int) or not 1 <= case["timeout_seconds"] <= 600:
            raise ValueError("Invalid bounded case deadline")
        if case.get("performance"):
            phases = case.get("phases")
            if not isinstance(phases, list) or not phases or any(
                not isinstance(phase, str) or not re.fullmatch(r"[a-z][a-z-]*", phase) for phase in phases
            ) or len(set(phases)) != len(phases):
                raise ValueError("Measured cases must declare distinct expected phases")
            if set(case.get("required_signals", {})) - set(phases):
                raise ValueError("Required signal has no declared phase")
        identities.add(case["id"])
    return manifest


def selected_cases(manifest, requested):
    cases = manifest["cases"]
    available = {case["id"] for case in cases}
    unknown = set(requested or []) - available
    if unknown:
        raise ValueError(f"Unknown cases: {sorted(unknown)}")
    return [case for case in cases if not requested or case["id"] in requested or case["id"] == "protein-first-interface"]


def installed_runtime():
    installed = shutil.which("lince")
    if installed is None:
        return None
    executable = Path(installed).resolve(strict=True)
    for candidate in (executable, executable.with_name(".lince-wrapped")):
        try:
            with candidate.open("rb") as source:
                if source.read(4) == b"\x7fELF":
                    return candidate
        except OSError:
            continue
    return None


def active_runtime():
    if not Path("/proc").is_dir():
        return None
    for process in Path("/proc").iterdir():
        try:
            if process.name.isdecimal() and process.stat().st_uid == os.getuid():
                candidate = (process / "exe").resolve(strict=True)
                if candidate.name not in {"lince", ".lince-wrapped"} or not str(candidate).startswith("/nix/store/"):
                    continue
                arguments = (process / "cmdline").read_bytes().split(b"\0")
                if b"--park" in arguments:
                    return candidate
        except (OSError, PermissionError):
            continue
    return None


def elf_runtime(path):
    with path.open("rb") as source:
        size = source.seek(0, 2)

        def read(offset, length):
            if offset < 0 or length < 0 or offset + length > size:
                raise ValueError("Truncated ELF runtime metadata")
            source.seek(offset)
            value = source.read(length)
            if len(value) != length:
                raise ValueError("Truncated ELF runtime metadata")
            return value

        header = read(0, 64)
        if header[:6] != b"\x7fELF\x02\x01":
            raise ValueError("Runtime must be a supported little-endian ELF64 executable")
        fields = struct.unpack("<16sHHIQQQIHHHHHH", header)
        offset, entry_size, entries = fields[5], fields[9], fields[10]
        if entry_size < 56 or entry_size > 4096 or not 1 <= entries <= 256:
            raise ValueError("Unsupported ELF program-header table")
        segments = [struct.unpack("<IIQQQQQQ", read(offset + index * entry_size, 56)) for index in range(entries)]
        interpreter, dynamic = None, {}
        for segment in segments:
            kind, _, position, _, _, length, _, _ = segment
            if kind == 3:
                if not 1 <= length <= 65536:
                    raise ValueError("Invalid ELF interpreter length")
                interpreter = read(position, length).split(b"\0", 1)[0].decode()
            elif kind == 2:
                if length > 1024 * 1024 or length % 16:
                    raise ValueError("Invalid bounded ELF dynamic table")
                for tag, value in struct.iter_unpack("<QQ", read(position, length)):
                    if tag == 0:
                        break
                    dynamic[tag] = value
        address, length = dynamic.get(5), dynamic.get(10, 0)
        path_offset = dynamic.get(29, dynamic.get(15))
        if not interpreter or address is None or path_offset is None or not 0 <= path_offset < length <= 1024 * 1024:
            raise ValueError("ELF runtime lacks interpreter or bounded library-path metadata")
        for kind, _, position, virtual, _, file_size, _, _ in segments:
            if kind == 1 and virtual <= address and address - virtual + length <= file_size:
                strings = read(position + address - virtual, length)
                value, separator, _ = strings[path_offset:].partition(b"\0")
                if not separator:
                    raise ValueError("Unterminated ELF runtime library path")
                return interpreter, value.decode()
        raise ValueError("ELF runtime string table is not in a file-backed load segment")


def runtime_prefix(requested):
    runtime = requested or active_runtime() or installed_runtime()
    if runtime is None:
        if Path("/etc/NIXOS").exists():
            raise ValueError("No installed Lince ELF runtime found; supply --runtime with the installed ELF path")
        return []
    runtime = runtime.resolve(strict=True)
    loader, paths = elf_runtime(runtime)
    paths = paths.replace("${ORIGIN}", str(runtime.parent)).replace("$ORIGIN", str(runtime.parent))
    if not Path(loader).is_file() or not paths:
        raise ValueError("Installed runtime has no usable loader/library path")
    return [loader, "--library-path", paths]


def process_resources(pid):
    try:
        root = Path("/proc") / str(pid)
        fields = (root / "stat").read_text().rsplit(") ", 1)[1].split()
        return {"observed_ns": time.monotonic_ns(), "cpu_ticks": int(fields[11]) + int(fields[12]),
                "rss_bytes": int(fields[21]) * os.sysconf("SC_PAGE_SIZE"), "start_ticks": int(fields[19])}
    except (OSError, ValueError, IndexError):
        return None


def filesystem_identity(path):
    selected, depth = {"type": "unavailable", "device": None}, -1
    try:
        for line in Path("/proc/self/mountinfo").read_text().splitlines():
            mount, filesystem = line.split(" - ", 1)
            fields = mount.split()
            decoded = re.sub(r"\\([0-7]{3})", lambda match: chr(int(match[1], 8)), fields[4])
            point = Path(decoded)
            if path.resolve().is_relative_to(point) and len(point.parts) > depth:
                selected, depth = {"type": filesystem.split()[0], "device": fields[2]}, len(point.parts)
    except (OSError, ValueError, IndexError):
        pass
    return selected


def host_identity():
    cpu_model = {}
    try:
        for line in Path("/proc/cpuinfo").read_text().split("\n\n", 1)[0].splitlines():
            key, separator, value = line.partition(":")
            if separator and key.strip() in {"vendor_id", "model name", "cpu family", "model", "stepping"}:
                cpu_model[key.strip()] = value.strip()
    except OSError:
        pass
    devices = []
    for node in sorted(Path("/sys/class/drm").glob("renderD*/device")):
        try:
            devices.append({"vendor": (node / "vendor").read_text().strip(),
                            "device": (node / "device").read_text().strip(),
                            "driver": (node / "driver").resolve(strict=True).name})
        except OSError:
            continue
    return {"system": os.uname().sysname, "kernel": os.uname().release, "machine": os.uname().machine,
            "cpu_count": os.cpu_count(), "cpu_model": cpu_model, "render_devices": devices,
            "graphics_environment": {key: os.environ[key] for key in (
                "WGPU_BACKEND", "WGPU_ADAPTER_NAME", "VK_DRIVER_FILES", "VK_ICD_FILENAMES", "LIBGL_ALWAYS_SOFTWARE"
            ) if key in os.environ}}


def phase_observations(path, case):
    values = {phase: summarize(path, phase) for phase in case["phases"]}
    for phase, requirements in case.get("required_signals", {}).items():
        value = values[phase]
        for stage in requirements.get("stages", []):
            if value["stages"].get("stage/" + stage, {}).get("count", 0) == 0:
                value["issues"].append("required-stage-unobserved:" + stage)
        for counter in requirements.get("positive_counters", []):
            if value["counters"].get(counter, 0) <= 0:
                value["issues"].append("required-work-unobserved:" + counter)
        value["complete"] = not value["issues"]
    return values


def execute_case(root, manifest, case, output, prefix, capture):
    artifact = manifest["artifacts"][case["artifact"]]
    executable = artifact_path(root, artifact["path"])
    command = prefix + [str(executable), case["selector"], "--exact", "--nocapture", "--test-threads=1"]
    if case.get("ignored"):
        command.append("--ignored")
    environment = os.environ.copy()
    environment.update({"RUST_MIN_STACK": "33554432", "LINCE_TRACE_PERFORMANCE": "1" if capture else "0"})
    environment.update(case.get("environment", {}))
    for key in ("LINCE_PARK_TEST_ORIGINAL_MAIN", "LINCE_PARK_TRACE_SYSTEMS"):
        environment.pop(key, None)
    output.mkdir(parents=True, exist_ok=True)
    stdout_path, stderr_path = output / "stdout.log", output / "stderr.log"
    performance_path = output / "performance.jsonl"
    temporary = output / "temporary"
    temporary.mkdir(mode=0o700)
    environment["TMPDIR"] = str(temporary)
    environment["LINCE_PARK_TEST_ARTIFACTS"] = str(output / "artifacts")
    environment["LINCE_PERFORMANCE_LOG"] = str(performance_path)
    if case.get("edit_probe"):
        environment["LINCE_EDIT_PROBE_OUTPUT"] = str(output / "edit-observations.json")
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr, env=environment, start_new_session=True)
        deadline = time.monotonic() + case["timeout_seconds"]
        failure = None
        resources = []
        try:
            while process.poll() is None:
                sample = process_resources(process.pid)
                if sample is not None:
                    resources.append(sample)
                if time.monotonic() >= deadline:
                    failure = "case-timeout"
                    break
                if stdout_path.stat().st_size + stderr_path.stat().st_size + (performance_path.stat().st_size if performance_path.exists() else 0) > MAX_LOG_BYTES:
                    failure = "bounded-log-capacity-exceeded"
                    break
                time.sleep(0.05)
        finally:
            if process.poll() is None:
                import signal
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
        status = process.wait()
    stdout = stdout_path.read_text(errors="replace") if stdout_path.stat().st_size <= MAX_LOG_BYTES else ""
    exact = "test result: ok. 1 passed;" in stdout
    result = {"case": case["id"], "selector": case["selector"], "scope": case["scope"],
              "exit": status, "passed": status == 0 and exact and failure is None,
              "failure": failure or (None if exact else "exact-selector-did-not-pass-one-test"),
              "artifact_sha256": artifact["sha256"], "stdout": str(stdout_path), "stderr": str(stderr_path)}
    if len(resources) >= 2 and len({value["start_ticks"] for value in resources}) == 1:
        first, last = resources[0], resources[-1]
        seconds = (last["observed_ns"] - first["observed_ns"]) / 1_000_000_000
        cpu_seconds = (last["cpu_ticks"] - first["cpu_ticks"]) / os.sysconf("SC_CLK_TCK")
        result["process_resources"] = {"samples": len(resources), "observation_seconds": seconds,
            "cpu_seconds": cpu_seconds, "mean_cpu_cores": cpu_seconds / seconds if seconds else None,
            "maximum_rss_bytes": max(value["rss_bytes"] for value in resources),
            "scope": "whole-private-test-process; sampled-before-exit; not-per-frame-or-gpu-time"}
    else:
        result["process_resources"] = {"status": "unavailable", "reason": "insufficient-stable-process-samples"}
    result["diagnostic_capture"] = capture
    if case.get("performance") and capture:
        result["performance"] = summarize(performance_path if performance_path.exists() else stderr_path)
        result["phases"] = phase_observations(performance_path if performance_path.exists() else stderr_path, case)
    if case.get("edit_probe") and result["passed"]:
        observation = load_json(output / "edit-observations.json")
        if observation.get("reopened_exactly") is not True or observation.get("journal") != "wal" or observation.get("synchronous") != 2 or len(observation.get("rounds", [])) != 3:
            raise ValueError("Private local edit did not establish durable exact reopen")
        result["edit_observations"] = observation
    if result["passed"] and case.get("observation_files"):
        result["observations"] = {name: load_json(artifact_path(output / "artifacts", name)) for name in case["observation_files"]}
    save_json(output / "result.json", result)
    return result


def run_suite(arguments):
    root = arguments.bundle.resolve()
    manifest = check_bundle(root)
    cases = selected_cases(manifest, arguments.case)
    prefix = runtime_prefix(arguments.runtime)
    output = arguments.output.resolve()
    if output.exists():
        raise ValueError("Use a new output directory so evidence cannot be overwritten")
    output.mkdir(parents=True, mode=0o700)
    filesystem = filesystem_identity(output)
    if arguments.capture == "on" and any(case.get("edit_probe") for case in cases) and filesystem["type"] in {"tmpfs", "ramfs", "unavailable"}:
        raise ValueError("Local-edit timing requires an identifiable disk-backed output directory; temporary fixture data stays beside the report")
    result = {"schema_version": 1, "revision": manifest["revision"], "manifest_sha256": digest(root / "manifest.json"),
              "comparison_identity": {**manifest["comparison_identity"], "host": host_identity(), "fixture_filesystem": filesystem}, "phases": {}, "checks": [],
              "unavailable": manifest.get("unavailable", [])}
    try:
        for case in cases:
            print(f"Running {case['id']}", flush=True)
            value = execute_case(root, manifest, case, output / case["id"], prefix, arguments.capture == "on")
            result["checks"].append(value)
            if "performance" in value:
                for phase, observation in value["phases"].items():
                    result["phases"][case["id"] + "/" + phase] = observation
            save_json(output / "run.json", result)
            if case["id"] == "protein-first-interface" and not value["passed"]:
                raise ValueError("Mandatory first interface gate failed; evidence retained")
    finally:
        save_json(output / "run.json", result)
    result["selected_checks_passed"] = all(value["passed"] for value in result["checks"])
    result["performance_complete"] = bool(result["phases"]) and all(phase["complete"] for phase in result["phases"].values())
    result["physical_acceptance"] = False
    save_json(output / "run.json", result)
    if not result["selected_checks_passed"]:
        raise ValueError("One or more selected checks failed; inspect run.json")
    if arguments.capture == "on" and any(case.get("performance") for case in cases) and not result["performance_complete"]:
        raise ValueError("Performance observations incomplete; inspect run.json and retained case logs")
    print(output / "run.json")


def list_tests(executable):
    result = subprocess.run([str(executable), "--list", "--format=terse"], capture_output=True, text=True, timeout=30, check=True)
    return [line.removesuffix(": test") for line in result.stdout.splitlines() if line.endswith(": test")]


def suite_cases():
    cases = [
        {"id": "protein-first-interface", "artifact": "interface-tests", "selector": "native::laboratory::interface::fast_interface_rules", "scope": "private-native-interface-correctness", "timeout_seconds": 120},
        {"id": "combined-native", "artifact": "park-tests", "selector": "presentation::tests::performance::combined::mixed_daily_phases_publish_measured_work_and_complete_pixels", "scope": "private-two-output-mixed-native", "timeout_seconds": 180, "ignored": True, "performance": True,
         "phases": ["warmup", "quiet", "typing", "scroll", "pan", "open-reveal", "styling", "resize", "description", "active", "return-quiet"],
         "required_signals": {"warmup": {"stages": ["native-update"], "positive_counters": ["node-metric-consumers-visited", "description-markup-renders"]},
                              "description": {"stages": ["native-description-markup"], "positive_counters": ["description-markup-renders"]}}},
        {"id": "gpu-passes", "artifact": "park-tests", "selector": "presentation::tests::performance::gpu::native_pass_hardware_intervals_belong_to_one_completed_full_or_pixel_frame", "scope": "private-native-hardware-query-coverage", "timeout_seconds": 180, "ignored": True, "environment": {"LINCE_PARK_GPU_DIAGNOSTICS": "1"}, "performance": True, "phases": ["warmup", "gpu-full", "gpu-pixel"], "observation_files": ["native-pass-hardware.json"]},
        {"id": "typing-pixels", "artifact": "park-tests", "selector": "presentation::tests::performance::typing::edit_panel_typing_paints_content_without_invalidating_text_geometry", "scope": "private-production-typing-and-rendered-glyph-oracle", "timeout_seconds": 180, "ignored": True, "performance": True, "phases": ["warmup", "typing", "reopen"]},
        {"id": "3d-capture", "artifact": "park-tests", "selector": "presentation::tests::capture::contributors::imported_glb_and_generated_wgsl_meshes_follow_their_real_sources_in_capture_and_a_replacement_keeps_its_public_pixels", "scope": "private-existing-3d-shader-resource-and-pixel-correctness; not-daily-3d-throughput", "timeout_seconds": 180, "ignored": True},
        {"id": "input-visible", "artifact": "park-tests", "selector": "runtime::latency::tests::visible::controlled_typing_changes_pixels_after_editor_application_and_keeps_its_input_origin", "scope": "private-input-to-verified-pixels", "timeout_seconds": 180, "ignored": True, "performance": True, "phases": ["warmup", "typing"], "observation_files": ["native-input-visible.json"]},
    ]
    for name, field, size, views, subscriptions in (
        ("title-one-view", "head", 1024, 1, 0),
        ("title-eight-views", "head", 1024, 8, 8),
        ("body-1k-one-view", "body", 1024, 1, 1),
        ("body-64k-one-view", "body", 65536, 1, 1),
        ("body-64k-eight-views", "body", 65536, 8, 8),
        ("body-1m-eight-views", "body", 1048576, 8, 8),
    ):
        cases.append({
            "id": "local-edit-" + name, "artifact": "interface-tests",
            "selector": "native::record_binding::performance_tests::file_backed_local_edit_phase_probe",
            "scope": "private-file-backed-binding-cell-query-refresh-exact-reopen; not-physical-pixels",
            "timeout_seconds": 300, "ignored": True, "performance": True, "edit_probe": True,
            "phases": ["warmup", "quiet", "local-save", "reopen"],
            "required_signals": {"local-save": {"stages": [
                "record-binding-update", "edit-probe-set-text", "edit-probe-mirror-observed-await",
                "edit-probe-durable-recovery-observed-await", "edit-probe-acknowledgement-observed-await",
                "edit-probe-refresh-settled-observed-await"]}, "reopen": {"stages": ["edit-probe-cold-reopen-await"]}},
            "environment": {"LINCE_EDIT_PROBE_BYTES": str(size), "LINCE_EDIT_PROBE_VIEWS": str(views),
                            "LINCE_EDIT_PROBE_FIELD": field, "LINCE_EDIT_PROBE_POSITION": "end",
                            "LINCE_EDIT_PROBE_SUBSCRIPTIONS": str(subscriptions)},
        })
    return cases


def package(arguments):
    if arguments.cargo_messages:
        artifacts_from_cargo = {}
        with arguments.cargo_messages.open() as source:
            for line in source:
                value = json.loads(line)
                if value.get("reason") == "compiler-artifact" and value.get("profile", {}).get("test") and value.get("executable"):
                    name = value["target"]["name"]
                    if name in {"lince_interface", "lince_park"}:
                        if name in artifacts_from_cargo and artifacts_from_cargo[name] != value["executable"]:
                            raise ValueError("Ambiguous test artifacts in Cargo output")
                        artifacts_from_cargo[name] = value["executable"]
        arguments.interface = Path(artifacts_from_cargo["lince_interface"])
        arguments.park = Path(artifacts_from_cargo["lince_park"])
    if arguments.interface is None or arguments.park is None:
        raise ValueError("Supply precompiled interface/Park tests or CI Cargo messages")
    if arguments.output.exists():
        raise ValueError("Bundle destination already exists")
    root = arguments.output.resolve()
    (root / "bin").mkdir(parents=True)
    artifacts = {}
    for name, original in (("interface-tests", arguments.interface), ("park-tests", arguments.park), ("application", arguments.application)):
        original = original.resolve()
        if not original.is_file():
            raise ValueError(f"Missing compiled {name}")
        target = root / "bin" / name
        shutil.copy2(original, target)
        artifacts[name] = {"path": str(target.relative_to(root)), "sha256": digest(target)}
    build = subprocess.run([str(arguments.application.resolve()), "--build-info"], capture_output=True, text=True, timeout=30, check=True)
    build_info = json.loads(build.stdout)
    if build_info.get("revision") != arguments.revision:
        raise ValueError("Application build revision differs from validation source")
    available = {"interface-tests": list_tests(root / "bin/interface-tests"), "park-tests": list_tests(root / "bin/park-tests")}
    cases = suite_cases()
    optional = []
    for case in cases:
        if case["selector"] not in available[case["artifact"]]:
            if case["id"] in {"protein-first-interface", "combined-native"} or case.get("edit_probe"):
                raise ValueError(f"Required selector absent from artifact: {case['selector']}")
            optional.append({"case": case["id"], "reason": "selector-unavailable-in-this-feature-build"})
    cases = [case for case in cases if case["selector"] in available[case["artifact"]]]
    fixture = Path(__file__).resolve().parents[3] / "crates/park/src/presentation/tests/performance/combined.rs"
    fixture_paths = [fixture] + [Path(__file__).resolve().parents[3] / path for path in (
        "crates/interface/src/native/record_binding/performance_tests.rs",
        "crates/park/src/presentation/tests/performance/gpu.rs",
        "crates/park/src/presentation/tests/performance/typing.rs",
        "crates/park/src/runtime/latency/tests/visible.rs",
    )]
    fixture_sha256 = hashlib.sha256("\n".join(digest(path) for path in fixture_paths).encode()).hexdigest()
    attribution = Path(__file__).resolve().parents[3] / "institute/anicca/park/measurement-changes.json"
    shutil.copy2(attribution, root / "changes.json")
    source = Path(__file__).parent
    for filename in ("performance.py", "report.py"):
        shutil.copy2(source / filename, root / filename)
        artifacts[filename] = {"path": filename, "sha256": digest(root / filename)}
    artifacts["change-attribution"] = {"path": "changes.json", "sha256": digest(root / "changes.json")}
    identity = {"workload": "park-a0-combined-v2", "workload_sha256": fixture_sha256, "quality": "normal", "scope": "private-native-two-output-plus-private-local-edit-and-separate-hardware-cases",
                "profile": arguments.profile, "target": arguments.target, "toolchain": arguments.toolchain,
                "features": "park/session,server-tests;interface/native-presentation"}
    save_json(root / "manifest.json", {"schema_version": 1, "revision": arguments.revision, "build_info": build_info,
              "comparison_identity": identity, "fixture_sha256": {str(path.relative_to(Path(__file__).resolve().parents[3])): digest(path) for path in fixture_paths}, "artifacts": artifacts, "cases": cases,
              "unavailable": optional + [{"case": "physical-daily-S3", "reason": "requires-separate-observation-of-installed-session-and-ordinary-external-windows"}]})
    check_bundle(root)
    print(root / "manifest.json")


def main():
    parser = argparse.ArgumentParser(description="Run and compare precompiled Park performance checks without building")
    subcommands = parser.add_subparsers(dest="command", required=True)
    run = subcommands.add_parser("run")
    run.add_argument("bundle", type=Path)
    run.add_argument("--output", type=Path, required=True)
    run.add_argument("--case", action="append")
    run.add_argument("--runtime", type=Path)
    run.add_argument("--capture", choices=("on", "off"), default="on", help="Enable measurement or collect whole-process overhead controls; production algorithms remain identical")
    summary = subcommands.add_parser("summarize")
    summary.add_argument("log", type=Path)
    summary.add_argument("--output", type=Path, required=True)
    comparison = subcommands.add_parser("compare")
    comparison.add_argument("before", type=Path)
    comparison.add_argument("after", type=Path)
    comparison.add_argument("--changes", type=Path, required=True)
    comparison.add_argument("--output", type=Path, required=True)
    bundle = subcommands.add_parser("package")
    for name in ("interface", "park"):
        bundle.add_argument("--" + name, type=Path)
    bundle.add_argument("--cargo-messages", type=Path)
    for name in ("application", "output"):
        bundle.add_argument("--" + name, type=Path, required=True)
    for name in ("revision", "profile", "target", "toolchain"):
        bundle.add_argument("--" + name, required=True)
    arguments = parser.parse_args()
    try:
        if arguments.command == "run":
            run_suite(arguments)
        elif arguments.command == "summarize":
            save_json(arguments.output, summarize(arguments.log))
        elif arguments.command == "compare":
            result = compare(load_json(arguments.before), load_json(arguments.after), load_json(arguments.changes))
            save_json(arguments.output, result)
            arguments.output.with_suffix(".txt").write_text(readable(result))
            print(readable(result), end="")
        else:
            package(arguments)
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as failure:
        parser.exit(1, f"Park measurement unavailable: {failure}\n")


if __name__ == "__main__":
    main()
