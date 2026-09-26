import subprocess

output = subprocess.check_output(
    ["cargo", "tree", "--locked", "-p", "lince-media", "--features", "native",
     "--edges", "normal,build", "--prefix", "none", "--format", "{p}|{f}"],
    text=True,
)
forbidden = {"webrtc-sys", "libwebrtc", "cxx", "cxx-build", "aws-lc-rs", "aws-lc-sys", "ring", "openssl-sys", "opus-sys", "audiopus_sys", "dav1d-sys", "libvpx-sys", "ffmpeg-sys-next"}
for line in output.splitlines():
    package, features = line.split("|", 1)
    name = package.split()[0]
    if name in forbidden:
        raise SystemExit(f"Native media dependency is forbidden: {name}")
    if name in {"rav1e", "rav1d"} and "asm" in features.split(","):
        raise SystemExit(f"Native assembly codec feature is forbidden: {line}")
print("Media transport, codecs, echo processing, and crypto have no native implementation dependencies.")
