// Stage 8b — the sand-settings "Data (Protein)" panel. This is how you make a
// sand receive data: pick a saved Protein from the list to drive the card, or
// build one with the GUI (no AST typing). Self-contained — owns its DOM subtree
// and multiplexes over the board transport. Picking/saving writes the card's
// widgetState via `patchCardState`, and the sand re-subscribes live.
//
// Deps: getCard(cardId) -> { id, widgetState }, patchCardState(cardId, patch),
// syncFrames().

import { getSharedTransport } from "./transport.js";

const LIST_PROTEIN = {
  source: "record",
  where: [{ all: [{ kind_eq: "protein" }, { quantity_gt: 0 }] }],
  limit: 200,
};

// Explicit "show everything" — NOT the same as clearing to
// `{ savedProtein: null, protein: null }`, which sands now read as "no data
// source configured yet" (2026-07-18 no-automatic-fallback change) and render
// as an empty/prompt state rather than all records.
const ALL_RECORDS_PROTEIN = { source: "record" };

// The record kinds and predicate/order vocabulary offered in the dropdowns.
const KINDS = ["plain", "rule", "signal", "transfer", "decision", "device", "organ", "person", "protein", "sand", "thread", "message"];
const SOURCES = ["record", "promise", "decision"];
const FILTERS = {
  record: [
    { field: "kind", label: "Kind" },
    { field: "slug", label: "Slug" },
    { field: "uid", label: "UID" },
    { field: "concept", label: "Concept" },
    { field: "organ", label: "Organ" },
    { field: "quantity", label: "Quantity" },
    { field: "text", label: "Text" },
    { field: "work_start", label: "Start date" },
    { field: "work_due", label: "Due date" },
    { field: "relation", label: "Relation" },
    { field: "assignee", label: "Assignee" },
  ],
  promise: [{ field: "state", label: "State" }, { field: "uid", label: "UID" }],
  decision: [],
};
const IS_OPERATORS = [
  { value: "is", label: "is" },
  { value: "is_not", label: "is not" },
];
const QUANTITY_OPERATORS = [
  { value: "eq", label: "=" }, { value: "neq", label: "≠" },
  { value: "gt", label: ">" }, { value: "gte", label: "≥" },
  { value: "lt", label: "<" }, { value: "lte", label: "≤" },
];
const TEXT_OPERATORS = [
  { value: "contains", label: "contains" },
  { value: "not_contains", label: "does not contain" },
];
const DATE_OPERATORS = [
  { value: "eq", label: "is" }, { value: "neq", label: "is not" },
  { value: "lt", label: "before" }, { value: "lte", label: "on or before" },
  { value: "gt", label: "after" }, { value: "gte", label: "on or after" },
  { value: "exists", label: "is set" }, { value: "not_exists", label: "is not set" },
];
const RELATION_OPERATORS = [
  { value: "is", label: "is" }, { value: "is_not", label: "is not" },
  { value: "exists", label: "exists" }, { value: "not_exists", label: "does not exist" },
];
const DIRECTIONS = [
  { value: "out", label: "outgoing" },
  { value: "in", label: "incoming" },
  { value: "both", label: "either direction" },
];
const SORT_DIRS = [
  { value: "desc", label: "high → low" },
  { value: "asc", label: "low → high" },
];
const RECORD_SORT_FIELDS = [
  ["head", "Title"], ["body", "Description"], ["slug", "Slug"],
  ["kind", "Kind"], ["quantity", "Quantity"], ["concept_name", "Concept name"],
  ["assignee_name", "Assignee name"], ["start_date", "Start date"], ["due_date", "Due date"],
  ["created_at", "Created time"], ["updated_at", "Updated time"],
].map(([value, label]) => ({ value, label }));

function slugify(name) {
  const base = String(name || "")
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9.]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return base || "protein-" + Date.now();
}

// Tiny DOM helper.
function h(tag, props, ...kids) {
  const el = document.createElement(tag);
  if (props) {
    for (const [k, v] of Object.entries(props)) {
      if (v == null) continue;
      if (k === "class") el.className = v;
      else if (k === "text") el.textContent = v;
      else if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2).toLowerCase(), v);
      else el.setAttribute(k, v);
    }
  }
  for (const c of kids.flat()) {
    if (c == null || c === false) continue;
    el.append(c.nodeType ? c : document.createTextNode(String(c)));
  }
  return el;
}

function dropdown(options, value, onChange, cls) {
  const sel = h("select", { class: cls || "startup-field__input protein-input", onchange: (e) => onChange(e.target.value) });
  for (const opt of options) {
    const o = typeof opt === "string" ? { value: opt, label: opt } : opt;
    const node = h("option", { value: o.value }, o.label);
    if (o.value === value) node.selected = true;
    sel.append(node);
  }
  return sel;
}

function blankBuilder() {
  return {
    slug: "",
    name: "",
    source: "record",
    // The wire format has one outer all/any group.  Keep that wrapper out of
    // the editor so the groups people create first are peers at indentation 0.
    filter: filterRoot("all", [group("all")]),
    include: { facts: false, factsLimit: 10, promises: false, links: false, linksKinds: [], availability: false },
    aggregate: { on: false, op: "sum", by: "concept" },
    sorts: [],
    limit: "",
  };
}

function group(op = "all", children = []) {
  return { node: "group", op, children };
}

function filterRoot(op = "all", children = []) {
  return { node: "root", op, children };
}

function condition(field = "kind") {
  return { node: "condition", field, operator: "is", value: field === "kind" ? "plain" : "", direction: "out", relationKind: "" };
}

// --- AST <-> builder ---------------------------------------------------------

function buildAst(b) {
  const ast = { source: b.source };
  const root = filterRootToPredicate(b.filter);
  if (root) ast.where = [root];

  const include = {};
  if (b.include.facts) include.facts = { limit: Number(b.include.factsLimit) || 10 };
  if (b.include.promises) include.promises = {};
  if (b.include.links) {
    // Many kinds (2026-07-18): each entry is one link kind, "*" = every kind.
    // The relations graph fans parallel kinds out as bent lines between the
    // same pair of nodes.
    const kinds = (Array.isArray(b.include.linksKinds) ? b.include.linksKinds : [])
      .map((kind) => String(kind || "").trim()).filter(Boolean);
    if (kinds.length) include.links = { kinds: [...new Set(kinds)] };
  }
  if (b.include.availability) include.availability = true;
  if (Object.keys(include).length) ast.include = include;

  if (b.aggregate.on) ast.aggregate = { op: b.aggregate.op, by: b.aggregate.by };

  const order = [];
  for (const s of b.sorts) {
    const field = String(s.field || "").trim();
    if (!field) continue;
    if (s.type === "link") {
      order.push({ link: { kind: field, higher: s.higher === "to" ? "to" : "from" } });
    } else {
      order.push(s.dir === "asc" ? { asc: field } : { desc: field });
    }
  }
  if (order.length) ast.order = order;

  if (b.limit !== "" && b.limit != null) ast.limit = Number(b.limit);
  return ast;
}

function negated(predicate, negative) {
  return negative ? { not: predicate } : predicate;
}

function conditionToPredicate(item) {
  const value = String(item.value ?? "").trim();
  if (item.field === "state") {
    const states = value.split(",").map((state) => state.trim()).filter(Boolean);
    return states.length ? negated({ state_in: states }, item.operator === "is_not") : null;
  }
  if (item.field === "quantity") {
    if (value === "" || !Number.isFinite(Number(value))) return null;
    const op = item.operator === "neq" ? "eq" : item.operator;
    return negated({ [`quantity_${op}`]: Number(value) }, item.operator === "neq");
  }
  if (item.field === "text") {
    return value ? negated({ text_contains: value }, item.operator === "not_contains") : null;
  }
  if (item.field === "work_start" || item.field === "work_due") {
    const negative = item.operator === "neq" || item.operator === "not_exists";
    const op = item.operator === "neq" ? "eq" : item.operator === "not_exists" ? "exists" : item.operator;
    if (op !== "exists" && !value) return null;
    return negated({ work_date: { field: item.field === "work_start" ? "start" : "due", op, ...(op === "exists" ? {} : { value }) } }, negative);
  }
  if (item.field === "relation" || item.field === "assignee") {
    const kind = item.field === "assignee" ? "assigned-to" : String(item.relationKind || "").trim();
    const exists = item.operator === "exists" || item.operator === "not_exists";
    if (!kind || (!exists && !value)) return null;
    const predicate = { relation: { kind, direction: item.direction || "out", ...(exists ? {} : { other: value }) } };
    return negated(predicate, item.operator === "is_not" || item.operator === "not_exists");
  }
  if (!value) return null;
  const keys = { kind: "kind_eq", slug: "slug_eq", uid: "uid_eq", concept: "concept_in", organ: "organ_eq" };
  const key = keys[item.field];
  return key ? negated({ [key]: value }, item.operator === "is_not") : null;
}

function groupToPredicate(node) {
  const children = node.children.map((child) => child.node === "group" ? groupToPredicate(child) : conditionToPredicate(child)).filter(Boolean);
  return children.length ? { [node.op === "any" ? "any" : "all"]: children } : null;
}

function filterRootToPredicate(root) {
  const children = root.children.map(groupToPredicate).filter(Boolean);
  return children.length ? { [root.op === "any" ? "any" : "all"]: children } : null;
}

function filterComplete(node, root = true) {
  if (node.children.length === 0) return root;
  return node.children.every((child) => child.node === "group"
    ? filterComplete(child, false)
    : Boolean(conditionToPredicate(child)));
}

function builderFromAst(name, slug, ast) {
  const b = blankBuilder();
  b.name = name || "";
  b.slug = slug || "";
  if (!ast || typeof ast !== "object") return b;
  b.source = SOURCES.includes(ast.source) ? ast.source : "record";
  const predicates = Array.isArray(ast.where) ? ast.where : [];
  if (predicates.length) {
    if (predicates.length !== 1 || (!predicates[0]?.all && !predicates[0]?.any)) {
      throw new Error("Saved Protein filter must have one root group");
    }
    // A canonical Protein always has one outer group. Its direct children are
    // the zero-indented groups in the editor, rather than one extra visible
    // level of indentation.
    const root = predicateToNode(predicates[0]);
    b.filter = filterRoot(root.op, root.children.map((child) => child.node === "group"
      ? child
      : group("all", [child])));
  }
  const inc = ast.include || {};
  if (inc.facts) { b.include.facts = true; b.include.factsLimit = inc.facts.limit ?? 10; }
  if (inc.promises) b.include.promises = true;
  if (inc.links) {
    b.include.links = true;
    const kinds = [...(Array.isArray(inc.links.kinds) ? inc.links.kinds : [])];
    b.include.linksKinds = [...new Set(kinds.map((kind) => String(kind || "").trim()).filter(Boolean))];
  }
  if (inc.availability) b.include.availability = true;
  if (ast.aggregate) b.aggregate = { on: true, op: ast.aggregate.op || "sum", by: ast.aggregate.by || "concept" };
  for (const o of Array.isArray(ast.order) ? ast.order : []) {
    const dir = Object.keys(o || {})[0];
    if (dir === "asc" || dir === "desc") b.sorts.push({ type: "field", dir, field: String(o[dir]) });
    if (dir === "link" && o.link?.kind) b.sorts.push({ type: "link", field: String(o.link.kind), higher: o.link.higher === "to" ? "to" : "from" });
  }
  if (ast.limit != null) b.limit = String(ast.limit);
  return b;
}


function predicateToNode(predicate, invert = false) {
  const key = Object.keys(predicate || {})[0];
  if (!key) throw new Error("Protein predicate is empty");
  if (key === "not") {
    const childKey = Object.keys(predicate.not || {})[0];
    if (childKey === "all" || childKey === "any") {
      throw new Error("Protein groups cannot be negated");
    }
    return predicateToNode(predicate.not, true);
  }
  if (key === "all" || key === "any") {
    return group(key, (predicate[key] || []).map((child) => predicateToNode(child)));
  }
  const negative = invert;
  const simple = { kind_eq: "kind", slug_eq: "slug", uid_eq: "uid", concept_in: "concept", organ_eq: "organ" };
  if (simple[key]) return { ...condition(simple[key]), value: String(predicate[key] ?? ""), operator: negative ? "is_not" : "is" };
  if (key.startsWith("quantity_")) {
    const op = key.slice("quantity_".length);
    const opposite = { eq: "neq", gt: "lte", gte: "lt", lt: "gte", lte: "gt" };
    return { ...condition("quantity"), value: String(predicate[key]), operator: negative ? opposite[op] : op };
  }
  if (key === "text_contains") return { ...condition("text"), value: String(predicate[key]), operator: negative ? "not_contains" : "contains" };
  if (key === "state_in") return { ...condition("state"), value: (predicate[key] || []).join(", "), operator: negative ? "is_not" : "is" };
  if (key === "relation") {
    const relation = predicate[key] || {};
    const assignee = relation.kind === "assigned-to";
    const exists = relation.other == null || relation.other === "";
    return { ...condition(assignee ? "assignee" : "relation"), relationKind: relation.kind || "", direction: relation.direction || "out", value: relation.other || "", operator: exists ? (negative ? "not_exists" : "exists") : (negative ? "is_not" : "is") };
  }
  if (key === "work_date") {
    const work = predicate[key] || {};
    const opposite = { exists: "not_exists", eq: "neq", lt: "gte", lte: "gt", gt: "lte", gte: "lt" };
    const op = negative ? opposite[work.op] : work.op;
    return { ...condition(work.field === "due" ? "work_due" : "work_start"), value: work.value || "", operator: op || "eq" };
  }
  throw new Error(`Unsupported Protein predicate: ${key}`);
}

export { buildAst, builderFromAst, filterComplete };

// --- panel -------------------------------------------------------------------

export function createProteinConfigPanel({ getCard, patchCardState, syncFrames }) {
  const listEl = document.getElementById("widget-config-protein-list");
  const builderEl = document.getElementById("widget-config-protein-builder");
  const help = document.getElementById("widget-config-protein-help");
  if (!listEl || !builderEl || !help) return { open() {}, close() {} };

  let cardId = null;
  let list = []; // [{ uid, slug, head, body }]
  let mode = "list"; // "list" | "edit"
  let builder = blankBuilder();
  // Link ordering deliberately offers only kinds that currently occur in a
  // link.  The subscription below is the Protein-facing equivalent of the
  // store's `used_kinds` query; it keeps the editor off an all-vocabulary list.
  let linkKinds = []; // [string]
  let people = []; // [{ uid, slug, head }]
  // Shares the board's single transport socket (Stage 8b, base task 1) instead
  // of opening its own. Our ids (`protein-list`, `pa-<n>`) never collide with
  // the widget bridge's (`<instanceId>:<subId>`, `act:<...>`), so filtering by
  // id keeps our messages ours.
  const transport = getSharedTransport();
  let nextReq = 1;
  let subscribed = false;
  const pending = new Map();
  let previewTimer = null;
  let previewId = null;

  function stopPreview() {
    window.clearTimeout(previewTimer);
    previewTimer = null;
    if (previewId) transport.send({ type: "unsubscribe", id: previewId });
    previewId = null;
  }

  function schedulePreview() {
    window.clearTimeout(previewTimer);
    previewTimer = window.setTimeout(() => {
      if (mode !== "edit" || builder.source !== "record") return;
      if (previewId) transport.send({ type: "unsubscribe", id: previewId });
      if (!filterComplete(builder.filter)) {
        previewId = null;
        const output = builderEl.querySelector("[data-protein-match-count]");
        if (output) output.textContent = "incomplete";
        return;
      }
      previewId = `protein-preview-${nextReq++}`;
      const ast = buildAst(builder);
      transport.send({ type: "subscribe", id: previewId, protein: {
        source: "record",
        ...(ast.where ? { where: ast.where } : {}),
        aggregate: { op: "count", by: "total" },
      } });
    }, 250);
  }

  function setHelp(text, isError) {
    help.textContent = text || "";
    help.hidden = !text;
    help.classList.toggle("is-ok", Boolean(text) && !isError);
  }

  function subscribeList() {
    transport.send({ type: "subscribe", id: "protein-list", protein: LIST_PROTEIN });
    transport.send({ type: "subscribe", id: "protein-link-kinds", protein: {
      source: "record", include: { links: { kinds: ["*"] } }, limit: 2000,
    } });
    transport.send({ type: "subscribe", id: "protein-people", protein: {
      source: "record", where: [{ kind_eq: "person" }], limit: 500,
    } });
  }

  function ensureSocket() {
    if (subscribed) return;
    subscribed = true;
    transport.onMessage((msg) => {
      if ((msg.type === "snapshot" || msg.type === "update") && msg.id === "protein-list") {
        list = (msg.rows || []).map((r) => ({ uid: r.uid, slug: r.slug, head: r.head, body: r.body }));
        if (mode === "list") renderList();
      } else if ((msg.type === "snapshot" || msg.type === "update") && msg.id === "protein-link-kinds") {
        linkKinds = [...new Set((msg.rows || []).flatMap((row) => (row.links || []).map((link) => link.kind)).filter(Boolean))].sort();
        if (mode === "edit") renderBuilder();
      } else if ((msg.type === "snapshot" || msg.type === "update") && msg.id === "protein-people") {
        people = (msg.rows || []).map((person) => ({ uid: person.uid, slug: person.slug, head: person.head }));
        if (mode === "edit" && builder.source === "record") renderBuilder();
      } else if ((msg.type === "snapshot" || msg.type === "update") && msg.id === previewId) {
        const count = (msg.rows || []).reduce((sum, row) => sum + Number(row.count || 0), 0);
        const output = builderEl.querySelector("[data-protein-match-count]");
        if (output) output.textContent = `${count} ${count === 1 ? "record" : "records"}`;
      } else if (msg.type === "action_ok" || msg.type === "error") {
        const resolve = pending.get(String(msg.id));
        if (resolve) { pending.delete(String(msg.id)); resolve(msg); }
      }
    });
    // Re-subscribe on every (re)connect; the Cell session is fresh each time.
    transport.onOpen(subscribeList);
    subscribeList();
  }

  function act(action) {
    return new Promise((resolve) => {
      ensureSocket();
      const id = "pa-" + nextReq++;
      pending.set(id, resolve);
      void transport.sendAction(id, action).catch((error) => {
        const finish = pending.get(id);
        if (!finish) return;
        pending.delete(id);
        finish({
          type: "error",
          id,
          message: error?.message || "Session signing is unavailable.",
          code: error?.code || "session_signing_unavailable",
        });
      });
      window.setTimeout(() => {
        if (pending.has(id)) { pending.delete(id); resolve({ type: "error", message: "timed out" }); }
      }, 15000);
    });
  }

  function currentDriver() {
    const ws = getCard(cardId)?.widgetState || {};
    if (ws.savedProtein) return { kind: "saved", value: String(ws.savedProtein) };
    if (ws.protein && typeof ws.protein === "object") return { kind: "all" };
    return { kind: "none" };
  }

  function drive(patch, label) {
    if (!cardId) return;
    patchCardState(cardId, patch);
    syncFrames();
    setHelp(label, false);
  }

  // ----- list view -----
  function renderList() {
    stopPreview();
    mode = "list";
    const driver = currentDriver();
    listEl.replaceChildren();

    const allRow = h("div", { class: "protein-item" + (driver.kind === "all" ? " is-active" : "") },
      h("button", { type: "button", class: "protein-item__pick",
        onclick: () => drive({ savedProtein: null, protein: ALL_RECORDS_PROTEIN }, "This card shows all records.") }, "All records"),
    );
    listEl.append(allRow);

    for (const p of list) {
      const active = driver.kind === "saved" && driver.value === p.slug;
      const row = h("div", { class: "protein-item" + (active ? " is-active" : "") },
        h("button", { type: "button", class: "protein-item__pick", title: "Show this in the card",
          onclick: () => drive({ savedProtein: p.slug, protein: null }, `This card shows "${p.head || p.slug}".`) },
          p.head || p.slug || "(unnamed)"),
        h("button", { type: "button", class: "protein-item__edit", title: "Edit",
          onclick: () => openEditor(p) }, "Edit"),
        h("button", { type: "button", class: "protein-item__del", title: "Delete",
          onclick: () => deleteProtein(p) }, "×"),
      );
      listEl.append(row);
    }

    builderEl.replaceChildren(
      h("button", { type: "button", class: "button button--accent protein-new",
        onclick: () => openEditor(null) }, "+ New Protein"),
    );
  }

  async function deleteProtein(p) {
    const res = await act({ action: "deactivate", target: p.slug });
    if (res.type === "error") { setHelp("Delete failed: " + (res.message || "rejected"), true); return; }
    if (currentDriver().value === p.slug) drive({ savedProtein: null, protein: ALL_RECORDS_PROTEIN }, `Deleted "${p.head || p.slug}". Showing all records.`);
    else setHelp(`Deleted "${p.head || p.slug}".`, false);
  }

  // ----- editor (GUI builder) -----
  function openEditor(existing) {
    setHelp("");
    if (existing) {
      let ast = {};
      try { ast = existing.body ? JSON.parse(existing.body) : {}; } catch { ast = {}; }
      builder = builderFromAst(existing.head, existing.slug, ast);
    } else {
      builder = blankBuilder();
    }
    mode = "edit";
    listEl.replaceChildren();
    renderBuilder();
  }

  function field(label, ...controls) {
    return h("label", { class: "protein-field" }, h("span", { class: "protein-field__label" }, label), ...controls);
  }

  function operatorsFor(item) {
    if (item.field === "quantity") return QUANTITY_OPERATORS;
    if (item.field === "text") return TEXT_OPERATORS;
    if (item.field === "work_start" || item.field === "work_due") return DATE_OPERATORS;
    if (item.field === "relation" || item.field === "assignee") return RELATION_OPERATORS;
    return IS_OPERATORS;
  }

  function renderFilterCondition(item, parent) {
    const specs = FILTERS[builder.source] || [];
    const fieldSelect = dropdown(specs.map((spec) => ({ value: spec.field, label: spec.label })), item.field, (value) => {
      Object.assign(item, condition(value)); renderBuilder();
    });
    const operatorSelect = dropdown(operatorsFor(item), item.operator, (value) => {
      item.operator = value; renderBuilder();
    });
    const controls = [fieldSelect, operatorSelect];

    if (item.field === "relation") {
      const kind = h("input", { class: "startup-field__input protein-input protein-input--sm", value: item.relationKind || "", placeholder: "kind", list: "protein-link-kinds" });
      kind.addEventListener("input", () => { item.relationKind = kind.value; schedulePreview(); });
      controls.push(kind, dropdown(DIRECTIONS, item.direction || "out", (value) => { item.direction = value; schedulePreview(); }));
    } else if (item.field === "assignee") {
      item.direction = "out";
    }

    const needsValue = !["exists", "not_exists"].includes(item.operator);
    if (needsValue) {
      const isDate = item.field === "work_start" || item.field === "work_due";
      const isNumber = item.field === "quantity";
      const value = item.field === "kind"
        ? dropdown(KINDS, item.value || "plain", (next) => { item.value = next; schedulePreview(); })
        : item.field === "assignee"
        ? dropdown([
            { value: "", label: "Choose person" },
            ...people.map((person) => ({ value: person.slug || person.uid, label: person.head || person.slug || person.uid })),
            ...(item.value && !people.some((person) => (person.slug || person.uid) === item.value)
              ? [{ value: item.value, label: item.value }] : []),
          ], item.value || "", (next) => { item.value = next; schedulePreview(); })
        : h("input", { class: "startup-field__input protein-input", type: isDate ? "date" : isNumber ? "number" : "text", value: item.value || "", placeholder: item.field === "assignee" ? "person slug or uid" : item.field === "relation" ? "record slug or uid" : "value" });
      if (value.tagName === "INPUT") value.addEventListener("input", () => { item.value = value.value; schedulePreview(); });
      controls.push(value);
    }
    controls.push(h("button", { type: "button", class: "protein-row__del", title: "Remove condition", onclick: () => {
      parent.children.splice(parent.children.indexOf(item), 1); renderBuilder();
    } }, "×"));
    return h("div", { class: "protein-row protein-condition" }, ...controls);
  }

  function renderFilterGroup(node, depth = 0, parent = null) {
    const body = h("div", { class: "protein-filter-group__body" });
    for (const child of node.children) {
      body.append(child.node === "group" ? renderFilterGroup(child, depth + 1, node) : renderFilterCondition(child, node));
    }
    const specs = FILTERS[builder.source] || [];
    const actions = h("div", { class: "protein-filter-group__actions" },
      h("button", { type: "button", class: "protein-add", disabled: specs.length ? null : "", onclick: () => {
        node.children.push(condition(specs[0]?.field || "kind")); renderBuilder();
      } }, "+ filter"),
      h("button", { type: "button", class: "protein-add", disabled: depth >= 10 || !specs.length ? "" : null,
        title: depth >= 10 ? "Maximum filter indentation is 10" : "Add nested group", onclick: () => {
          if (depth >= 10 || !specs.length) return;
          node.children.push(group("all", [condition(specs[0]?.field || "kind")])); renderBuilder();
        } }, "+ subgroup"),
      parent ? h("button", { type: "button", class: "protein-row__del", title: "Remove group", onclick: () => {
        parent.children.splice(parent.children.indexOf(node), 1); renderBuilder();
      } }, "×") : null);
    return h("div", { class: `protein-filter-group depth-${Math.min(depth, 10)}` },
      h("div", { class: "protein-filter-group__head" },
        h("span", { class: "protein-field__label" }, "Group"),
        dropdown([{ value: "all", label: "AND" }, { value: "any", label: "OR" }], node.op, (value) => { node.op = value; schedulePreview(); }),
        actions),
      body);
  }

  function renderFilterRoot(root) {
    const body = h("div", { class: "protein-filter-group__body" });
    for (const node of root.children) body.append(renderFilterGroup(node, 0, root));
    const specs = FILTERS[builder.source] || [];
    return h("div", { class: "protein-filter-root" },
      h("div", { class: "protein-filter-group__head" },
        h("span", { class: "protein-field__label" }, "Match groups"),
        dropdown([{ value: "all", label: "AND" }, { value: "any", label: "OR" }], root.op, (value) => { root.op = value; schedulePreview(); }),
        h("div", { class: "protein-filter-group__actions" },
          h("button", { type: "button", class: "protein-add", disabled: !specs.length ? "" : null,
            title: "Add a zero-indented group", onclick: () => {
              if (!specs.length) return;
              root.children.push(group("all", [condition(specs[0].field)]));
              renderBuilder();
            } }, "+ group"))),
      body);
  }

  function renderBuilder() {
    const b = builder;
    const nameInput = h("input", { class: "startup-field__input protein-input", type: "text",
      placeholder: "Name (e.g. Stock levels)", value: b.name });
    nameInput.addEventListener("input", () => { b.name = nameInput.value; });

    const filtersWrap = renderFilterRoot(b.filter);

    // includes
    const inc = b.include;
    const check = (label, checked, on) => {
      const cb = h("input", { type: "checkbox" });
      cb.checked = checked;
      cb.addEventListener("change", () => { on(cb.checked); renderBuilder(); });
      return h("label", { class: "protein-check" }, cb, h("span", null, label));
    };
    const factsLimit = h("input", { class: "startup-field__input protein-input protein-input--sm", type: "number", value: inc.factsLimit });
    factsLimit.addEventListener("input", () => { inc.factsLimit = factsLimit.value; });
    // Links include: any number of link kinds (each with the autocomplete
    // datalist; "*" = every kind). One protein can therefore pull several
    // link types at once — the relations graph draws them as fanned-out
    // bent lines between the same two nodes.
    const linksKindsWrap = h("div", { class: "protein-rows" });
    inc.linksKinds.forEach((kind, i) => {
      const kindInput = h("input", { class: "startup-field__input protein-input", type: "text",
        placeholder: "link kind (* = all)", value: kind, list: "protein-link-kinds" });
      kindInput.addEventListener("input", () => { inc.linksKinds[i] = kindInput.value; });
      linksKindsWrap.append(h("div", { class: "protein-row" },
        kindInput,
        h("button", { type: "button", class: "protein-row__del", onclick: () => { inc.linksKinds.splice(i, 1); renderBuilder(); } }, "×"),
      ));
    });
    const linksKindsField = h("div", { class: "protein-group" },
      h("div", { class: "protein-group__head" }, "of kinds",
        h("button", { type: "button", class: "protein-add",
          onclick: () => { inc.linksKinds.push(""); renderBuilder(); } }, "+ kind")),
      linksKindsWrap);

    // sorts
    const sortsWrap = h("div", { class: "protein-rows" });
    b.sorts.forEach((s, i) => {
      const isLink = s.type === "link";
      const fieldInput = isLink
        ? dropdown([{ value: "", label: "Choose link kind" }, ...linkKinds.map((kind) => ({ value: kind, label: kind }))], s.field || "", (v) => { s.field = v; schedulePreview(); })
        : dropdown(builder.source === "record" ? RECORD_SORT_FIELDS : [{ value: "uid", label: "UID" }], s.field || "uid", (v) => { s.field = v; schedulePreview(); });
      const row = h("div", { class: "protein-row", draggable: "true",
        ondragstart: (event) => event.dataTransfer.setData("text/plain", String(i)),
        ondragover: (event) => event.preventDefault(),
        ondrop: (event) => { event.preventDefault(); const from = Number(event.dataTransfer.getData("text/plain")); if (Number.isInteger(from) && from !== i) { const [item] = b.sorts.splice(from, 1); b.sorts.splice(i, 0, item); renderBuilder(); } },
      },
        h("span", { title: "Drag to set priority", class: "protein-sort-handle" }, "⠿"),
        fieldInput,
        isLink
          ? h("span", { class: "protein-link-order" }, "A — @", s.field || "kind", " → B",
            dropdown([{ value: "from", label: "A higher" }, { value: "to", label: "B higher" }], s.higher || "from", (v) => { s.higher = v; renderBuilder(); }))
          : h("span", null,
            h("button", { type: "button", title: "Ascending", onclick: () => { s.dir = "asc"; renderBuilder(); } }, "↑"),
            h("button", { type: "button", title: "Descending", onclick: () => { s.dir = "desc"; renderBuilder(); } }, "↓")),
        h("button", { type: "button", class: "protein-row__del", onclick: () => { b.sorts.splice(i, 1); renderBuilder(); } }, "×"),
      );
      sortsWrap.append(row);
    });

    const limitInput = h("input", { class: "startup-field__input protein-input protein-input--sm", type: "number", placeholder: "no limit", value: b.limit });
    limitInput.addEventListener("input", () => { b.limit = limitInput.value; });

    builderEl.replaceChildren(
      field("Name", nameInput),
      field("Show", dropdown(SOURCES.map((s) => ({ value: s, label: s === "record" ? "records" : s === "promise" ? "promises" : "decisions" })), b.source, (v) => {
        b.source = v;
        b.filter = filterRoot("all", [group("all")]);
        renderBuilder();
      })),
      h("div", { class: "protein-group" },
        h("div", { class: "protein-group__head" }, "Filters",
          b.source === "record" ? h("span", { "data-protein-match-count": "", class: "protein-match-count" }, "… records") : null),
        filtersWrap),
      h("div", { class: "protein-group" },
        h("div", { class: "protein-group__head" }, "Include extra info"),
        h("div", { class: "protein-checks" },
          check("history (facts)", inc.facts, (v) => { inc.facts = v; }),
          inc.facts ? field("how many", factsLimit) : null,
          check("promises", inc.promises, (v) => { inc.promises = v; }),
          check("availability", inc.availability, (v) => { inc.availability = v; }),
          check("links", inc.links, (v) => {
            inc.links = v;
            if (v && !inc.linksKinds.length) inc.linksKinds.push(""); // show one input right away
          }),
          inc.links ? linksKindsField : null,
        )),
      h("div", { class: "protein-group" },
        h("div", { class: "protein-group__head" }, "Aggregate",
          (() => {
            const cb = h("input", { type: "checkbox" }); cb.checked = b.aggregate.on;
            cb.addEventListener("change", () => { b.aggregate.on = cb.checked; renderBuilder(); });
            return h("label", { class: "protein-check" }, cb, h("span", null, "on"));
          })()),
        b.aggregate.on ? h("div", { class: "protein-row" },
          dropdown([{ value: "sum", label: "sum" }, { value: "count", label: "count" }], b.aggregate.op, (v) => { b.aggregate.op = v; }),
          h("span", { class: "protein-field__label" }, "by"),
          dropdown([{ value: "concept", label: "concept" }, { value: "kind", label: "kind" }], b.aggregate.by, (v) => { b.aggregate.by = v; }),
        ) : null),
      h("div", { class: "protein-group" },
        h("div", { class: "protein-group__head" }, "Sort",
          h("button", { type: "button", class: "protein-add", onclick: () => { b.sorts.push({ type: "field", field: "head", dir: "asc" }); renderBuilder(); } }, "+ field"),
          b.source === "record" ? h("button", { type: "button", class: "protein-add", onclick: () => { b.sorts.push({ type: "link", field: "", higher: "from" }); renderBuilder(); } }, "+ link") : null),
        sortsWrap),
      field("Limit", limitInput),
      h("div", { class: "protein-actions" },
        h("button", { type: "button", class: "button button--accent", onclick: () => void save() }, "Save"),
        h("button", { type: "button", class: "button button--ghost", onclick: () => renderList() }, "Cancel"),
      ),
      h("datalist", { id: "protein-link-kinds" }, ...linkKinds.map((n) => h("option", { value: n }))),
    );
    schedulePreview();
  }

  async function save() {
    const name = String(builder.name || "").trim();
    if (!name) { setHelp("Give it a name.", true); return; }
    if (!filterComplete(builder.filter)) { setHelp("Complete or remove every filter condition.", true); return; }
    const slug = builder.slug || slugify(name);
    const ast = buildAst(builder);
    const res = await act({ action: "save-protein", slug, head: name, ast });
    if (res.type === "error") { setHelp("Save failed: " + (res.message || "rejected"), true); return; }
    // saved: drive this card with it and return to the list
    drive({ savedProtein: slug, protein: null }, `Saved "${name}" and showing it in this card.`);
    renderList();
  }

  return {
    open(id) {
      cardId = id;
      setHelp("");
      ensureSocket();
      renderList();
    },
    close() {
      stopPreview();
      cardId = null;
      mode = "list";
    },
  };
}
