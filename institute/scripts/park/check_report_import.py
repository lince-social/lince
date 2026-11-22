"""Guide the P104 report import check in a disposable nested Park session."""

import argparse
from itertools import chain
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


REPOSITORY = Path(__file__).resolve().parents[3]
SAMPLE_REVISION = "0123456789abcdef0123456789abcdef01234567"
MAX_REPORT_BYTES = 16 * 1024 * 1024


def report_summary(path):
    if not path.is_file():
        raise ValueError("Choose a regular JSON or JSONL report file")
    with path.open("rb") as source:
        data = source.read(MAX_REPORT_BYTES + 1)
    if len(data) > MAX_REPORT_BYTES:
        raise ValueError("Park reports must be at most 16 MiB")
    for candidate in chain((data,), reversed(data.splitlines())):
        try:
            summary = json.loads(candidate)
        except (ValueError, UnicodeError):
            continue
        if isinstance(summary, dict) and summary.get("suite") == "park-responsiveness-v1":
            return summary
    raise ValueError("No final Park responsiveness report found")


def outer_wayland_socket():
    display = os.environ.get("WAYLAND_DISPLAY")
    if not display:
        raise ValueError("Run this from a terminal in a Wayland desktop")
    path = Path(display)
    if not path.is_absolute():
        runtime = os.environ.get("XDG_RUNTIME_DIR")
        if not runtime:
            raise ValueError("The parent Wayland desktop has no XDG_RUNTIME_DIR")
        path = Path(runtime) / path
    if not path.is_socket():
        raise ValueError("The parent Wayland socket is unavailable")
    return path


def check(args):
    executable = shutil.which(args.lince)
    bus = shutil.which("dbus-run-session")
    timeout = shutil.which("timeout")
    if not executable or not bus or not timeout:
        raise ValueError("Lince, dbus-run-session, and timeout must be available")
    info = json.loads(subprocess.check_output([executable, "--build-info"], timeout=10))
    if not info.get("features", {}).get("park"):
        raise ValueError("The chosen Lince executable does not include Park")
    socket = outer_wayland_socket()
    result_root = REPOSITORY / "target"
    result_root.mkdir(exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="park-p104-", dir=result_root))
    log_path = output / "nested-park.log"

    with tempfile.TemporaryDirectory(prefix="park-p104-") as temporary:
        private = Path(temporary)
        runtime = private / "run"
        runtime.mkdir(mode=0o700)
        config = private / "config"
        config.mkdir(mode=0o700)
        path = private / "report.jsonl"
        if args.report:
            report_summary(args.report)
            shutil.copyfile(args.report, path)
        else:
            sample = {
                "suite": "park-responsiveness-v1",
                "revision": SAMPLE_REVISION,
                "optimized": True,
                "measurements_delivered": False,
                "coverage": {"passed": False},
                "cases": [{"case": "empty", "passed": True}],
            }
            path.write_text(json.dumps(sample) + "\n")
        summary = report_summary(path)

        print(f"Park build: {info['revision']}", flush=True)
        print("In the nested window, open Edit → Information (ⓘ) → Laboratory.")
        print("Type or paste this path into Park report path, then click Load Park report.")
        print(f"  {path}")
        print(f"Check for Measured revision {summary.get('revision')}.")
        if args.report:
            print("Check that the reported case results and any unavailable metrics match the file.")
        else:
            print("Check for Incomplete · optimized, Empty workspace · passed, and Idle canvas worker and input · unavailable.")
        print("Close the nested window once you have checked the report.", flush=True)
        environment = os.environ.copy()
        environment.update(
            HOME=str(private),
            XDG_RUNTIME_DIR=str(runtime),
            XDG_CONFIG_HOME=str(config),
            XDG_CACHE_HOME=str(private / "cache"),
            XDG_DATA_HOME=str(private / "local-data"),
            XDG_STATE_HOME=str(private / "state"),
            WAYLAND_DISPLAY=str(socket),
        )
        for key in ("DISPLAY", "WAYLAND_SOCKET", "LINCE_PARK_UWSM"):
            environment.pop(key, None)
        command = [
            timeout, "--kill-after=5s", f"{args.seconds + 30}s",
            bus, "--", executable, "--park-nested",
            "--directory", str(private / "data"), "--peer-port", "0",
            "--language", "en", "--park-duration", str(args.seconds),
        ]
        with log_path.open("wb") as log:
            run = subprocess.run(command, env=environment, stdout=log, stderr=subprocess.STDOUT)

    if run.returncode:
        raise ValueError(f"Nested Park exited with {run.returncode}. See {log_path}")
    try:
        outcome = input("Did the imported report show the expected results? [y/N] ").strip().lower()
    except EOFError:
        outcome = ""
    status = {"y": "passed", "yes": "passed", "n": "failed", "no": "failed"}.get(
        outcome, "unconfirmed"
    )
    receipt = output / "result.json"
    receipt.write_text(json.dumps({
        "revision": info["revision"],
        "report_revision": summary.get("revision"),
        "report": "supplied" if args.report else "synthetic",
        "result": status,
    }, indent=2) + "\n")
    print(f"Result: {receipt}")
    return 0 if status == "passed" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, help="Use an existing report instead of the deterministic sample")
    parser.add_argument("--lince", default="lince", help="Installed Park executable")
    parser.add_argument("--seconds", type=int, default=300, help="Maximum duration of the nested session")
    args = parser.parse_args()
    if args.seconds < 1:
        parser.error("--seconds must be positive")
    try:
        return check(args)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
