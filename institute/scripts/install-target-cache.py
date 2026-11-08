import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]


def unit_argument(value):
    value = str(value).replace("%", "%%").replace("$", "$$")
    return json.dumps(value, ensure_ascii=False)


def units(executable, target):
    condition = str(target).replace("%", "%%")
    if any(character in condition for character in "\n\r\0"):
        raise ValueError("Target path contains unsupported control characters")
    service = f"""[Unit]
Description=Lince Cargo target cache budget (300 GB)
ConditionPathIsDirectory={condition}

[Service]
Type=oneshot
ExecStart={unit_argument(executable)} {unit_argument(target)}
Nice=19
IOSchedulingClass=idle
CPUWeight=1
"""
    timer = """[Unit]
Description=Keep Lince compiled artifacts within their target cache budget

[Timer]
OnStartupSec=3min
OnUnitInactiveSec=5min
AccuracySec=30s

[Install]
WantedBy=timers.target
"""
    return {"lince-target-cache.service": service, "lince-target-cache.timer": timer}


def install():
    executable = Path.home() / ".local/libexec/lince-target-cache"
    executable.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=executable.parent, prefix=".target-cache-") as temporary:
        compiled = Path(temporary) / "lince-target-cache"
        subprocess.run(["rustc", "--edition", "2024", "-D", "warnings", "-O", "-C", "strip=symbols",
                        str(ROOT / "institute/scripts/target-cache.rs"), "-o", str(compiled)], check=True, cwd=ROOT)
        os.replace(compiled, executable)
    directory = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "systemd/user"
    directory.mkdir(parents=True, exist_ok=True)
    for name, value in units(executable, ROOT / "target").items():
        with tempfile.NamedTemporaryFile(mode="w", dir=directory, prefix=".lince-target-cache-", delete=False) as output:
            output.write(value)
            temporary = Path(output.name)
        temporary.chmod(0o644)
        os.replace(temporary, directory / name)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(["systemctl", "--user", "enable", "--now", "lince-target-cache.timer"], check=True)
    subprocess.run(["systemctl", "--user", "start", "lince-target-cache.service"], check=True)
    print(f"Installed 300 GB target cache budget for {ROOT / 'target'}")


def main():
    parser = argparse.ArgumentParser(description="Install the local 300 GB Cargo target-cache timer without compiling Lince")
    parser.parse_args()
    install()


if __name__ == "__main__":
    main()
