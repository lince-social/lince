from collections import Counter
from itertools import combinations, product


def factor_pairs(values):
    return set(combinations(sorted(values.items()), 2))


def plan(workloads, available_cases):
    spec = workloads["workflow_coverage"]
    factors = spec["factors"]
    if not 2 <= len(factors) <= 20 or any(
        not 1 <= len(values) <= 12 or len(set(values)) != len(values)
        for values in factors.values()
    ):
        raise ValueError("Invalid finite workflow factors")
    universe = {
        ((first, left), (second, right))
        for first, second in combinations(sorted(factors), 2)
        for left, right in product(factors[first], factors[second])
    }
    exclusions = {}
    for exclusion in spec["inapplicable"]:
        pairs = factor_pairs(exclusion["factors"])
        if len(pairs) != 1:
            raise ValueError("An inapplicable combination must name exactly two factors")
        pair = pairs.pop()
        if pair not in universe or pair in exclusions or not exclusion["reason"]:
            raise ValueError("Invalid or duplicated inapplicable workflow pair")
        exclusions[pair] = exclusion["reason"]

    requirements = {item["id"]: item for item in spec["required"]}
    if len(requirements) != len(spec["required"]):
        raise ValueError("Duplicate required workflow combination")
    workflow_ids = set(workloads["required_workflows"])
    if any(not set(item["workflows"]) <= workflow_ids for item in requirements.values()):
        raise ValueError("Unknown workflow on a required combination")

    rows = {}
    selectors = spec["selectors"]
    for row in spec["rows"]:
        identity = row["id"]
        values = row["factors"]
        pairs = factor_pairs(values)
        if (identity in rows or row["case"] not in selectors or not pairs or not pairs <= universe
                or pairs.intersection(exclusions) or not row["oracle"]
                or not row["workflows"] or not set(row["workflows"]) <= workflow_ids
                or not set(row.get("required", ())) <= requirements.keys()):
            raise ValueError(f"Invalid workflow row: {identity}")
        rows[identity] = {**row, "selector": selectors[row["case"]]["selector"], "pairs": pairs}

    # A row covers only the factor values exercised together by its named oracle.
    # Select existing fixtures greedily, without inventing rows or scheduling missing oracles.
    available = {case["id"]: case["selector"] for case in available_cases}
    compatible = {
        case["id"] for case in available_cases
        if (expected := selectors.get(case["id"]))
        and expected["selector"] == case["selector"]
        and all(case.get("environment", {}).get(key) == value for key, value in expected.get("environment", {}).items())
    }
    by_case = {}
    for row in rows.values():
        if row["case"] in compatible:
            covered = by_case.setdefault(row["case"], set())
            covered.update(("pair", pair) for pair in row["pairs"])
            covered.update(("required", item) for item in row.get("required", ()))
    selected = ["protein-first-interface"]
    remaining = set().union(*by_case.values()) - by_case.pop(selected[0], set())
    while remaining:
        best = min(by_case, key=lambda name: (-len(remaining & by_case[name]), name))
        selected.append(best)
        remaining.difference_update(by_case.pop(best))
    return {
        "identity": workloads["identity"],
        "source": spec.get("source", "workflow registry"),
        "scope": spec.get("scope", "private fixtures"),
        "factors": factors,
        "universe": universe,
        "inapplicable": exclusions,
        "required": requirements,
        "rows": rows,
        "available": available,
        "compatible": compatible,
        "selected": selected,
    }


def report(coverage, checks):
    observed = {}
    for check in checks:
        identity = check["case"]
        if (identity not in coverage["available"] or identity in observed
                or check.get("selector") != coverage["available"][identity]):
            raise ValueError(f"Unknown, mismatched or duplicated workflow result: {identity}")
        observed[identity] = "passed" if check.get("passed") is True else "failed"

    rows = []
    pair_status = {}
    requirement_status = {}
    for row in coverage["rows"].values():
        if row["case"] not in coverage["compatible"]:
            status = "selector_unavailable"
        else:
            status = observed.get(row["case"], "not_run")
        rows.append({key: value for key, value in row.items() if key != "pairs"} | {"status": status})
        for pair in row["pairs"]:
            pair_status.setdefault(pair, []).append((row["id"], status))
        for requirement in row.get("required", ()):
            requirement_status.setdefault(requirement, []).append((row["id"], status))

    def result(evidence):
        order = ("failed", "passed", "not_run", "selector_unavailable")
        status = next((state for state in order if any(
            actual == state for _, actual in evidence
        )), "missing_oracle")
        return {"status": status, "rows": [identity for identity, _ in evidence]}

    pairs = []
    for pair in sorted(coverage["universe"]):
        if reason := coverage["inapplicable"].get(pair):
            evidence = {"status": "inapplicable", "reason": reason, "rows": []}
        else:
            evidence = result(pair_status.get(pair, ()))
        pairs.append({"factors": dict(pair), **evidence})
    required = [
        {**definition, **result(requirement_status.get(identity, ()))}
        for identity, definition in coverage["required"].items()
    ]
    statuses = Counter(pair["status"] for pair in pairs)
    complete = (
        all(pair["status"] in {"passed", "inapplicable"} for pair in pairs)
        and all(item["status"] == "passed" for item in required)
        and all(observed.get(case) == "passed" for case in coverage["selected"])
    )
    return {
        "schema_version": 1,
        "workload": coverage["identity"],
        "source": coverage["source"],
        "scope": coverage["scope"],
        "factors": coverage["factors"],
        "selected_cases": coverage["selected"],
        "case_results": observed,
        "rows": rows,
        "pairs": pairs,
        "pair_counts": dict(statuses),
        "required": required,
        "complete": complete,
    }
