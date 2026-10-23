import json
import sys
from pathlib import Path


def check(actual, expected=None):
    if not isinstance(actual, dict):
        raise ValueError("build information must be an object")
    for field in ("version", "revision", "target"):
        if not isinstance(actual.get(field), str) or not actual[field]:
            raise ValueError(f"missing build {field}")
    if not actual["target"].endswith("-linux-gnu"):
        raise ValueError("Park requires a supported Linux target")
    if actual.get("optimized") is not True:
        raise ValueError("the installed Linux GUI must use an optimized build")
    features = actual.get("features")
    if not isinstance(features, dict) or set(features) != {"ui", "instinct", "park"} or not all(value is True for value in features.values()):
        raise ValueError("Linux GUI executable must include ui, instinct and park")
    if expected is not None and actual != expected:
        raise ValueError("installed executable and archive build information differ")


if __name__ == "__main__":
    actual = json.loads(Path(sys.argv[1]).read_text())
    expected = json.loads(Path(sys.argv[2]).read_text()) if len(sys.argv) == 3 else None
    check(actual, expected)
