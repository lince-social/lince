"""Reconcile the native entry points with their declared Park workflow owners."""

from collections import Counter
import hashlib
import json
from pathlib import Path
import re


NATIVE = "crates/interface/src/native/"
APP = NATIVE + "app.rs"
CATALOGUE = NATIVE + "laboratory/catalogue.rs"
STORE = NATIVE + "sand_store.rs"
TRAIL = "institute/trails/lince/park/"
REGISTRY = TRAIL + "source-ownership.json"


def native_plugins(source):
    constructor = source.split("pub fn presentation_app(", 1)[1].split("\npub fn canvas(", 1)[0]
    calls = re.findall(r"\.add_plugins\((\([^()]+\)|[\w:]+)\)", constructor)
    if len(calls) != constructor.count(".add_plugins("):
        raise ValueError("Unsupported registration in presentation_app; update the inventory reader")
    return [name.strip() for call in calls for name in call.strip("()").split(",") if name.strip()]


def sand_kinds(source):
    declaration = source.split("pub enum SandKind {", 1)[1].split("\n}", 1)[0]
    choices = source.split("pub const ALL:", 1)[1].split("\n    ];", 1)[0]
    kinds = re.findall(r"^    (\w+),$", declaration, re.MULTILINE)
    if kinds != re.findall(r"Self::(\w+),", choices):
        raise ValueError("SandKind and the published Store choices differ")
    return kinds


def laboratory_registrations(source):
    return re.findall(r"^    ([\w:]+)\(&mut cases\);", source, re.MULTILINE)


def module_of(symbol):
    parts = symbol.removeprefix("crate::").split("::")
    return parts[1] if parts[0] in {"native", "controls"} else parts[0]


def audit(repo, prepared_cases):
    """Read source only. Case presence does not claim execution or performance."""
    sources = {}

    def read(name):
        if not name.startswith(("crates/", "institute/", ".github/")) or ".." in Path(name).parts:
            raise ValueError(f"Inventory path outside declared source roots: {name}")
        if name not in sources:
            sources[name] = (repo / name).read_bytes()
        return sources[name].decode()

    def anchors(items):
        for path, token in items:
            if not token or token not in read(path):
                raise ValueError(f"Missing inventory anchor {token!r} in {path}")

    registry = json.loads(read(REGISTRY))
    if registry["schema_version"] != 1:
        raise ValueError("Unsupported Park source inventory schema")
    anchors(registry["shipping"]["source"])
    anchors(registry["conditional"]["source"])
    families = registry["families"]
    if len({family["id"] for family in families}) != len(families):
        raise ValueError("Duplicate Park source inventory family")
    module_owners = {}
    sand_owners = {}
    known_workflows = set(json.loads(read(TRAIL + "workloads.json"))["required_workflows"])
    known_cases = {case["id"] for case in prepared_cases}
    for family in families:
        identifier = family["id"]
        if not family["workflows"] or set(family["workflows"]) - known_workflows:
            raise ValueError(f"Invalid workflow in {identifier}")
        for key in ("input", "data", "update", "workers", "persistence", "hidden"):
            if not family[key]:
                raise ValueError(f"Missing {key} ownership in {identifier}")
        evidence = family.get("selectors", [])
        prepared = family.get("prepared", [])
        if set(prepared) - known_cases or not (evidence or prepared or family.get("gap")):
            raise ValueError(f"Missing or unknown evidence in {identifier}")
        anchors(evidence)
        anchors(family.get("openers", []))
        for key, owners in (("modules", module_owners), ("sands", sand_owners)):
            for name in family.get(key, []):
                if name in owners:
                    raise ValueError(f"Duplicate owners for {name}: {owners[name]} and {identifier}")
                owners[name] = identifier

    registrations = []

    def collect(kind, path, names, owner_for):
        if duplicates := [name for name, count in Counter(names).items() if count > 1]:
            raise ValueError(f"Repeated {kind} registration in {path}: {duplicates}")
        for name in names:
            if (owner := owner_for(name)) is None:
                raise ValueError(f"Unassigned {kind} registration {name} in {path}")
            registrations.append({"kind": kind, "file": path, "entry": name, "owner": owner})

    collect("plugin", APP, native_plugins(read(APP)), lambda name: module_owners.get(module_of(name)))
    kinds = sand_kinds(read(STORE))
    if set(sand_owners) != set(kinds):
        raise ValueError("Store ownership and the SandKind catalogue differ")
    collect("sand", STORE, kinds, sand_owners.get)
    collect("laboratory", CATALOGUE, laboratory_registrations(read(CATALOGUE)),
            lambda name: module_owners.get(module_of(name)))

    constructor = "crate::native::sand_store::castle_entry("
    for path in sorted((repo / NATIVE).rglob("*.rs")):
        count = path.read_bytes().count(constructor.encode())
        if count:
            relative = path.relative_to(repo).as_posix()
            top_module = relative.removeprefix(NATIVE).split("/", 1)[0].removesuffix(".rs")
            collect("castle-store", relative, [f"castle_entry#{index + 1}" for index in range(count)],
                    lambda _: module_owners.get(top_module))
            read(relative)

    for group in (registry["shipping"], registry["conditional"]):
        for flag in group["entries"]:
            if not any(flag in (entry["entry"], entry["owner"]) for entry in registrations):
                raise ValueError(f"Missing registration cited by feature route: {flag}")

    return {
        "schema_version": 1,
        "shipping": registry["shipping"],
        "conditional": registry["conditional"],
        "families": families,
        "registrations": registrations,
        "files_sha256": {path: hashlib.sha256(content).hexdigest() for path, content in sorted(sources.items())},
    }
