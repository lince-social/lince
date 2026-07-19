import { formatQuantity, primaryStatus, statusLabel } from "./model.js";

const collapsed = new Set();

export function branchRows(rows, branchUid) {
  const byUid = new Map(rows.map((row) => [row.uid, row]));
  const result = [];
  const pending = branchUid ? [branchUid] : [];
  const visited = new Set();
  while (pending.length) {
    const uid = pending.shift();
    if (!uid || visited.has(uid)) continue;
    visited.add(uid);
    const row = byUid.get(uid);
    if (!row) continue;
    result.push(row);
    pending.push(...childUids(row, rows));
  }
  return result;
}

export function rootUidFor(rows, branchUid) {
  const byUid = new Map(rows.map((row) => [row.uid, row]));
  let current = byUid.get(branchUid);
  const visited = new Set();
  while (current && !visited.has(current.uid)) {
    visited.add(current.uid);
    if (current.hierarchy?.root && byUid.has(current.hierarchy.root)) return current.hierarchy.root;
    const parent = current.hierarchy?.parent || current.parent;
    if (!parent || !byUid.has(parent)) return current.uid;
    current = byUid.get(parent);
  }
  return branchUid || "";
}

export function renderHierarchyOverview(container, rows, visibleRows, state, options) {
  container.classList.add("hierarchyOverview");
  const roots = hierarchyRoots(rows);
  const selectedRoot = roots.some((row) => row.uid === state.rootUid) ? state.rootUid : "";
  const toolbar = el("header", "hierarchyToolbar");
  const identity = el("div", "hierarchyToolbarIdentity");
  identity.append(
    el("strong", "", "Transfer hierarchy"),
    el("span", "", `${rows.length} transfers · ${roots.length} root${roots.length === 1 ? "" : "s"}`),
  );
  const picker = el("select", "hierarchyRootSelect");
  picker.setAttribute("aria-label", "Select hierarchy root");
  picker.append(new Option("All trees", ""));
  for (const root of roots) picker.append(new Option(root.head, root.uid));
  picker.value = selectedRoot;
  picker.addEventListener("change", () => options.onRoot?.(picker.value));
  toolbar.append(identity, picker);

  const allowed = visibleHierarchyUids(rows, visibleRows);
  const tree = el("ol", "hierarchyTree");
  const shownRoots = selectedRoot ? roots.filter((row) => row.uid === selectedRoot) : roots;
  const visited = new Set();
  for (const root of shownRoots) appendTreeNode(tree, root, rows, allowed, state.branchUid, options, visited, 0);
  if (!tree.childElementCount) tree.append(el("li", "emptyInline", "No branches match this view"));
  container.replaceChildren(toolbar, tree);
}

export function renderHierarchyInspector(row, rows, onSelect, viewer = null) {
  const section = el("section", "detailSection hierarchyInspector");
  const heading = el("div", "hierarchyInspectorHeading");
  heading.append(el("h3", "", "Branch inspection"), status(readiness(row).ready ? "ready" : "blocked"));
  section.append(heading);

  const picker = el("select", "hierarchyBranchSelect");
  picker.setAttribute("aria-label", "Inspect another hierarchy branch");
  for (const item of flattenedHierarchy(rows)) {
    picker.append(new Option(`${"-- ".repeat(item.depth)}${item.row.head}`, item.row.uid));
  }
  picker.value = row.uid;
  picker.addEventListener("change", () => onSelect?.(picker.value));
  section.append(picker);

  const path = hierarchyPath(row, rows);
  section.append(el("div", "hierarchyPath", path.map((item) => item.head).join(" / ") || row.head));
  const facts = el("dl", "hierarchyInspectorFacts");
  const remainders = branchRemainders(row);
  facts.append(
    fact("Branch status", statusLabel(primaryStatus(row, viewer))),
    fact("Readiness", readiness(row).ready ? "Ready" : "Blocked"),
    fact("Remainder", remainders.length ? remainders.map(remainderLabel).join(" · ") : "No projected remainder"),
    fact("Children", String(childUids(row, rows).length)),
    fact("Dependency order", row.phase6?.dependency_order?.join(" → ") || "No projected order"),
    fact("Satiation", firstCompletesLabel(row.first_completes)),
  );
  section.append(facts);
  const blockers = branchBlockers(row);
  if (blockers.length) section.append(el("div", "hierarchyBlockers", blockers.map(blockerLabel).join(" · ")));
  return section;
}

function appendTreeNode(list, row, rows, allowed, selectedUid, options, visited, depth) {
  if (visited.has(row.uid) || !allowed.has(row.uid)) return;
  visited.add(row.uid);
  const children = childUids(row, rows)
    .map((uid) => rows.find((candidate) => candidate.uid === uid))
    .filter((child) => child && allowed.has(child.uid))
    .sort(compareRows);
  const item = el("li", "hierarchyTreeItem");
  item.style.setProperty("--tree-depth", String(depth));
  const node = el("div", "hierarchyTreeNode");
  node.dataset.selected = String(row.uid === selectedUid);
  if (children.length) {
    const toggle = el("button", "hierarchyToggle", collapsed.has(row.uid) ? "+" : "−");
    toggle.type = "button";
    toggle.setAttribute("aria-label", collapsed.has(row.uid) ? `Expand ${row.head}` : `Collapse ${row.head}`);
    toggle.addEventListener("click", () => {
      if (collapsed.has(row.uid)) collapsed.delete(row.uid);
      else collapsed.add(row.uid);
      options.onRefresh?.();
    });
    node.append(toggle);
  } else {
    node.append(el("span", "hierarchyTogglePlaceholder", ""));
  }
  const select = el("button", "hierarchyBranchButton");
  select.type = "button";
  select.addEventListener("click", () => options.onSelect?.(row.uid));
  const identity = el("span", "hierarchyNodeIdentity");
  identity.append(el("strong", "", row.head), el("span", "", row.slug || row.uid));
  const metrics = el("span", "hierarchyNodeMetrics");
  metrics.append(status(primaryStatus(row, options.viewer)), status(readiness(row).ready ? "ready" : "blocked"));
  const remainder = branchRemainders(row);
  metrics.append(el("span", "hierarchyRemainder", remainder.length
    ? remainder.slice(0, 2).map(remainderLabel).join(" · ")
    : "No remainder"));
  const blockers = branchBlockers(row);
  if (blockers.length) metrics.append(el("span", "hierarchyBlockerCount", `${blockers.length} blocker${blockers.length === 1 ? "" : "s"}`));
  select.append(identity, metrics);
  node.append(select);
  item.append(node);
  list.append(item);
  if (!children.length || collapsed.has(row.uid)) return;
  const childList = el("ol", "hierarchyChildren");
  for (const child of children) appendTreeNode(childList, child, rows, allowed, selectedUid, options, visited, depth + 1);
  item.append(childList);
}

function hierarchyRoots(rows) {
  const ids = new Set(rows.map((row) => row.uid));
  const roots = rows.filter((row) => {
    const parent = row.hierarchy?.parent || row.parent;
    return !parent || !ids.has(parent);
  }).sort(compareRows);
  return roots.length || !rows.length ? roots : [...rows].sort(compareRows);
}

function flattenedHierarchy(rows) {
  const result = [];
  const visited = new Set();
  function visit(row, depth) {
    if (!row || visited.has(row.uid)) return;
    visited.add(row.uid);
    result.push({ row, depth });
    for (const uid of childUids(row, rows)) visit(rows.find((candidate) => candidate.uid === uid), depth + 1);
  }
  for (const root of hierarchyRoots(rows)) visit(root, 0);
  for (const row of rows) visit(row, 0);
  return result;
}

function visibleHierarchyUids(rows, visibleRows) {
  if (visibleRows.length === rows.length) return new Set(rows.map((row) => row.uid));
  const byUid = new Map(rows.map((row) => [row.uid, row]));
  const allowed = new Set();
  for (const visible of visibleRows) {
    let current = visible;
    const visited = new Set();
    while (current && !visited.has(current.uid)) {
      visited.add(current.uid);
      allowed.add(current.uid);
      current = byUid.get(current.hierarchy?.parent || current.parent);
    }
  }
  return allowed;
}

function childUids(row, rows) {
  if (Array.isArray(row.hierarchy?.children) && row.hierarchy.children.length) return row.hierarchy.children;
  return rows.filter((candidate) => (candidate.hierarchy?.parent || candidate.parent) === row.uid).map((candidate) => candidate.uid);
}

function hierarchyPath(row, rows) {
  const byUid = new Map(rows.map((item) => [item.uid, item]));
  if (row.hierarchy?.path?.length) return row.hierarchy.path.map((uid) => byUid.get(uid)).filter(Boolean);
  const path = [];
  let current = row;
  const visited = new Set();
  while (current && !visited.has(current.uid)) {
    visited.add(current.uid);
    path.unshift(current);
    current = byUid.get(current.hierarchy?.parent || current.parent);
  }
  return path;
}

function branchRemainders(row) {
  const rollup = row.hierarchy?.rollup || {};
  const raw = rollup.remaining_by_resource || row.settlement_progress?.by_resource || [];
  if (Array.isArray(raw)) return raw.map(normalizeRemainder).filter((item) => item.remaining_quantity != null);
  return Object.entries(raw && typeof raw === "object" ? raw : {}).map(([key, value]) => normalizeRemainder({ concept_name: key, remaining_quantity: value }));
}

function normalizeRemainder(item) {
  const row = item && typeof item === "object" ? item : {};
  const remaining = Number(row.remaining_quantity ?? row.remaining ?? row.quantity);
  return {
    concept: row.concept_name || row.concept || "Unclassified",
    unit: row.unit_name || row.unit || "",
    remaining_quantity: Number.isFinite(remaining) ? remaining : null,
  };
}

function remainderLabel(item) {
  return `${item.concept}: ${formatQuantity(item.remaining_quantity, item.unit)}`;
}

function readiness(row) {
  if (row.phase6 && typeof row.phase6.ready === "boolean") {
    return { ready: row.phase6.ready, blockers: row.phase6.blockers || [] };
  }
  return row.readiness || { ready: false, blockers: [] };
}

function branchBlockers(row) {
  if (Array.isArray(row.phase6?.blockers) && row.phase6.blockers.length) return row.phase6.blockers;
  return Array.isArray(readiness(row).blockers) ? readiness(row).blockers : [];
}

function firstCompletesLabel(value) {
  if (!value) return "Not grouped";
  return [value.state && statusLabel(value.state), value.winner && `winner ${value.winner}`].filter(Boolean).join(" · ") || "Pending";
}

function blockerLabel(value) {
  if (value && typeof value === "object") return statusLabel(value.message || value.code || value.kind || "blocked");
  return statusLabel(value || "blocked");
}

function compareRows(left, right) {
  const leftOrder = Number(left.hierarchy?.order ?? left.phase6?.order ?? Number.MAX_SAFE_INTEGER);
  const rightOrder = Number(right.hierarchy?.order ?? right.phase6?.order ?? Number.MAX_SAFE_INTEGER);
  return leftOrder - rightOrder || left.head.localeCompare(right.head);
}

function fact(label, value) {
  const item = el("div", "fact");
  item.append(el("dt", "", label), el("dd", "", value == null || value === "" ? "None" : value));
  return item;
}

function status(value) {
  const pill = el("span", "status", statusLabel(value));
  pill.dataset.status = value || "active";
  return pill;
}

function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}
