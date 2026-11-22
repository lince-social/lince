"""Run P134 R08 with the matching prebuilt Park capture fixture."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from performance import RuntimePrefix, artifact_path, digest, execute_case, runtime_prefix, suite_cases
from report import load_json, save_json


REPOSITORY = Path(__file__).resolve().parents[3]
CASE_ID = "held-capture-source-privacy"


def check(args):
    bundle = args.bundle.resolve(strict=True)
    packaged = bundle / "manifest.json"
    staged = bundle / "tests.json"
    if packaged.is_file():
        manifest = load_json(packaged)
        artifact = manifest["artifacts"]["park-tests"]
        identity = manifest["build_info"]
    elif staged.is_file():
        manifest = load_json(staged)
        artifact = manifest["artifacts"]["lince_park"]
        identity = {"optimized": manifest.get("profile") == "release", "target": manifest.get("target")}
    else:
        raise ValueError("Expected an extracted validation bundle with manifest.json or tests.json")

    installed = json.loads(subprocess.check_output(["lince", "--build-info"], timeout=10))
    revision = installed["revision"]
    if manifest.get("schema_version") != 1 or not installed.get("features", {}).get("park"):
        raise ValueError("Expected a Park validation bundle and a Park-enabled Lince installation")
    if manifest.get("revision") != revision or identity.get("target") != installed.get("target"):
        raise ValueError(f"The fixture must match the installed Park revision and target ({revision})")
    if identity.get("optimized") is not True or installed.get("optimized") is not True:
        raise ValueError("The fixture and installed Park binary must use the optimized profile")
    executable = artifact_path(bundle, artifact["path"])
    if digest(executable) != artifact["sha256"]:
        raise ValueError("The Park fixture checksum does not match its manifest")

    bus = shutil.which("dbus-run-session")
    if not bus:
        raise ValueError("dbus-run-session is required for the private fixture")
    prefix = runtime_prefix(args.runtime)
    case = next(case for case in suite_cases() if case["id"] == CASE_ID)
    result_root = REPOSITORY / "target"
    result_root.mkdir(exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="park-r08-", dir=result_root))
    print(f"Park build: {revision}")
    print(f"Running the held-capture fixture. Results: {output}", flush=True)
    fixture = {"artifacts": {"park-tests": artifact}}
    with tempfile.TemporaryDirectory(prefix="park-r08-") as temporary:
        private = Path(temporary)
        private_prefix = RuntimePrefix(
            [bus, "--", *prefix],
            {
                **getattr(prefix, "environment", {}),
                "HOME": str(private),
                "XDG_RUNTIME_DIR": str(private),
                "XDG_CONFIG_HOME": str(private / "config"),
                "XDG_DATA_HOME": str(private / "data"),
                "XDG_CACHE_HOME": str(private / "cache"),
                "XDG_STATE_HOME": str(private / "state"),
                "DISPLAY": "",
                "WAYLAND_DISPLAY": "",
            },
        )
        result = execute_case(bundle, fixture, case, output, private_prefix, capture=False)
    save_json(output / "run.json", {"revision": revision, "check": result})
    print(f"Result: {output / 'run.json'}")
    if not result["passed"]:
        raise ValueError(f"Capture privacy was not verified. See {output / 'stdout.log'} and {output / 'stderr.log'}")
    print("Passed: the retired capture was rejected without writing pixels into the client buffer.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path, help="Extracted validation bundle or staged test directory")
    parser.add_argument("--runtime", type=Path, help="Installed Lince ELF for its graphics libraries")
    try:
        check(parser.parse_args())
    except (OSError, KeyError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
