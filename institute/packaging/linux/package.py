import argparse
import hashlib
import json
import runpy
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path


def package(executable, destination):
    repository = Path(__file__).resolve().parents[3]
    if executable.with_name(".lince-wrapped").exists():
        raise ValueError("package the compiled executable before Nix wrapping")
    with executable.open("rb") as binary:
        if binary.read(4) != b"\x7fELF":
            raise ValueError("package the compiled ELF executable, not a Nix wrapper")
    information = json.loads(subprocess.check_output([str(executable), "--build-info"], timeout=10))
    checker = runpy.run_path(str(Path(__file__).with_name("check-build-info.py")))
    checker["check"](information)
    if information.get("optimized") is not True:
        raise ValueError("the Linux release archive requires an optimized executable")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent, prefix="park-package-") as temporary:
        root = Path(temporary) / "lince"
        root.mkdir()
        files = {
            executable: "bin/lince",
            repository / "LICENSE": "LICENSE",
            repository / "institute/packaging/linux/lince-park-session": "bin/lince-park-session",
            repository / "institute/packaging/linux/wayland-sessions/lince.desktop": "share/wayland-sessions/lince.desktop",
            repository / "crates/vendor/smithay-capture/LICENSE.txt": "share/licenses/lince-smithay-capture/LICENSE.txt",
            repository / "crates/vendor/smithay-capture/CREDITS.md": "share/licenses/lince-smithay-capture/CREDITS.md",
            repository / "institute/packaging/linux/ufw/lince": "etc/ufw/applications.d/lince",
            repository / "institute/packaging/linux/firewalld/lince.xml": "lib/firewalld/services/lince.xml",
            repository / "institute/packaging/linux/firewalld/lince-test.xml": "lib/firewalld/services/lince-test.xml",
        }
        for source, relative in files.items():
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            target.chmod(0o755 if relative.startswith("bin/") else 0o644)
        (root / "build-info.json").write_text(json.dumps(information, indent=2) + "\n")
        (root / "revision").write_text(information["revision"] + "\n")
        with tarfile.open(destination, "w:gz", compresslevel=1) as archive:
            archive.add(root, arcname="lince")
    with destination.open("rb") as archive:
        digest = hashlib.file_digest(archive, "sha256").hexdigest()
    destination.with_name(destination.name + ".sha256").write_text(f"{digest}  {destination.name}\n")
    destination.with_name(destination.name + ".target").write_text(information["target"] + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("executable", type=Path)
    parser.add_argument("destination", type=Path)
    arguments = parser.parse_args()
    package(arguments.executable.resolve(), arguments.destination.resolve())
