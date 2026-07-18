// Stage 8b — the sand-settings "Data (Protein)" panel. This is how you make a
// sand receive data: pick a saved Protein from the list to drive the card, or
// build one with the GUI (no AST typing). Self-contained — owns its DOM subtree
// and a dedicated transport WebSocket. Picking/saving writes the card's
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
const FILTERS = [
  { type: "kind_eq", label: "kind is", input: "kind" },
  { type: "slug_eq", label: "slug is", input: "text" },
  { type: "uid_eq", label: "uid is", input: "text" },
  { type: "concept_in", label: "concept in", input: "text" },
  { type: "organ_eq", label: "organ is", input: "text" },
  { type: "quantity_gt", label: "quantity >", input: "number" },
  { type: "quantity_gte", label: "quantity >=", input: "number" },
  { type: "quantity_lt", label: "quantity <", input: "number" },
  { type: "quantity_lte", label: "quantity <=", input: "number" },
  { type: "quantity_eq", label: "quantity =", input: "number" },
  { type: "state_in", label: "state in (comma)", input: "text" },
  // Link predicates (K4 base-UI work, 2026-07-17): cluster-tag / graph
  // filtering is the Data panel's job, never sand chrome. `assignee` is
  // sugar for linked_to kind=assigned-to.
  { type: "linked_to", label: "linked to", input: "pair" },
  { type: "assignee", label: "assignee is", input: "text" },
];
const SORT_DIRS = [
  { value: "desc", label: "high → low" },
  { value: "asc", label: "low → high" },
  { value: "topo", label: "graph order (link kind)" },
];

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
    filters: [],
    include: { facts: false, factsLimit: 10, promises: false, links: false, linksKinds: [], availability: false },
    aggregate: { on: false, op: "sum", by: "concept" },
    sorts: [],
    limit: "",
  };
}

// --- AST <-> builder ---------------------------------------------------------

function buildAst(b) {
  const ast = { source: b.source };
  const where = [];
  for (const f of b.filters) {
    if (f.type === "state_in") {
      const states = String(f.value || "").split(",").map((s) => s.trim()).filter(Boolean);
      if (states.length) where.push({ state_in: states });
    } else if (f.type === "linked_to") {
      const kind = String(f.kind || "").trim();
      const to = String(f.to || "").trim();
      if (kind && to) where.push({ linked_to: { kind, to } });
    } else if (f.type === "assignee") {
      if (String(f.value || "").trim()) where.push({ linked_to: { kind: "assigned-to", to: String(f.value).trim() } });
    } else if (f.type.startsWith("quantity_")) {
      if (f.value !== "" && f.value != null) where.push({ [f.type]: Number(f.value) });
    } else if (String(f.value || "").trim()) {
      where.push({ [f.type]: String(f.value).trim() });
    }
  }
  if (where.length) ast.where = where;

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
    order.push(s.dir === "topo" ? { topo: field } : s.dir === "asc" ? { asc: field } : { desc: field });
  }
  if (order.length) ast.order = order;

  if (b.limit !== "" && b.limit != null) ast.limit = Number(b.limit);
  return ast;
}

function builderFromAst(name, slug, ast) {
  const b = blankBuilder();
  b.name = name || "";
  b.slug = slug || "";
  if (!ast || typeof ast !== "object") return b;
  b.source = SOURCES.includes(ast.source) ? ast.source : "record";
  for (const pred of Array.isArray(ast.where) ? ast.where : []) {
    const key = Object.keys(pred || {})[0];
    if (!key) continue;
    if (key === "state_in") b.filters.push({ type: "state_in", value: (pred[key] || []).join(", ") });
    else if (key === "linked_to") {
      const kind = String(pred.linked_to?.kind || "");
      const to = String(pred.linked_to?.to || "");
      if (kind === "assigned-to") b.filters.push({ type: "assignee", value: to });
      else b.filters.push({ type: "linked_to", kind, to });
    }
    else if (FILTERS.some((f) => f.type === key)) b.filters.push({ type: key, value: String(pred[key]) });
  }
  const inc = ast.include || {};
  if (inc.facts) { b.include.facts = true; b.include.factsLimit = inc.facts.limit ?? 10; }
  if (inc.promises) b.include.promises = true;
  if (inc.links) {
    b.include.links = true;
    // Round-trip both spellings: `kinds` (multi) and the legacy single `kind`.
    const kinds = [...(Array.isArray(inc.links.kinds) ? inc.links.kinds : [])];
    if (inc.links.kind) kinds.unshift(inc.links.kind);
    b.include.linksKinds = [...new Set(kinds.map((kind) => String(kind || "").trim()).filter(Boolean))];
  }
  if (inc.availability) b.include.availability = true;
  if (ast.aggregate) b.aggregate = { on: true, op: ast.aggregate.op || "sum", by: ast.aggregate.by || "concept" };
  for (const o of Array.isArray(ast.order) ? ast.order : []) {
    const dir = Object.keys(o || {})[0];
    if (dir === "topo" || dir === "asc" || dir === "desc") b.sorts.push({ dir, field: String(o[dir]) });
  }
  if (ast.limit != null) b.limit = String(ast.limit);
  return b;
}

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
  // Link kinds (and concept_in) are Lingua vocabulary (blueprint III), i.e.
  // concept slugs, not a fixed enum — offered here as free-text autocomplete
  // rather than a dropdown, since the catalog is open-ended and user-grown.
  let conceptNames = []; // [string]
  // Shares the board's single transport socket (Stage 8b, base task 1) instead
  // of opening its own. Our ids (`protein-list`, `pa-<n>`) never collide with
  // the widget bridge's (`<instanceId>:<subId>`, `act:<...>`), so filtering by
  // id keeps our messages ours.
  const transport = getSharedTransport();
  let nextReq = 1;
  let subscribed = false;
  const pending = new Map();

  function setHelp(text, isError) {
    help.textContent = text || "";
    help.hidden = !text;
    help.classList.toggle("is-ok", Boolean(text) && !isError);
  }

  function subscribeList() {
    transport.send({ type: "subscribe", id: "protein-list", protein: LIST_PROTEIN });
    transport.send({ type: "subscribe", id: "protein-concepts", protein: { source: "concept" } });
  }

  function ensureSocket() {
    if (subscribed) return;
    subscribed = true;
    transport.onMessage((msg) => {
      if ((msg.type === "snapshot" || msg.type === "update") && msg.id === "protein-list") {
        list = (msg.rows || []).map((r) => ({ uid: r.uid, slug: r.slug, head: r.head, body: r.body }));
        if (mode === "list") renderList();
      } else if ((msg.type === "snapshot" || msg.type === "update") && msg.id === "protein-concepts") {
        conceptNames = (msg.rows || []).map((r) => r.name).filter(Boolean);
        if (mode === "edit") renderBuilder();
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
      transport.send({ type: "act", id, action });
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

  function renderBuilder() {
    const b = builder;
    const nameInput = h("input", { class: "startup-field__input protein-input", type: "text",
      placeholder: "Name (e.g. Stock levels)", value: b.name });
    nameInput.addEventListener("input", () => { b.name = nameInput.value; });

    // filters
    const filtersWrap = h("div", { class: "protein-rows" });
    b.filters.forEach((f, i) => {
      const spec = FILTERS.find((x) => x.type === f.type) || FILTERS[0];
      const valueControl = spec.input === "kind"
        ? dropdown(KINDS, f.value || "plain", (v) => { f.value = v; })
        : spec.input === "pair"
        ? (() => {
            const kindInp = h("input", { class: "startup-field__input protein-input protein-input--sm",
              type: "text", value: f.kind || "", placeholder: "link kind (tag)", list: "protein-link-kinds" });
            kindInp.addEventListener("input", () => { f.kind = kindInp.value; });
            const toInp = h("input", { class: "startup-field__input protein-input",
              type: "text", value: f.to || "", placeholder: "to (slug/uid)" });
            toInp.addEventListener("input", () => { f.to = toInp.value; });
            return h("span", { class: "protein-pair", style: "display:flex;gap:4px;flex:1;min-width:0" }, kindInp, toInp);
          })()
        : (() => {
            const inp = h("input", { class: "startup-field__input protein-input",
              type: spec.input === "number" ? "number" : "text", value: f.value || "",
              placeholder: spec.label });
            inp.addEventListener("input", () => { f.value = inp.value; });
            return inp;
          })();
      filtersWrap.append(h("div", { class: "protein-row" },
        dropdown(FILTERS.map((x) => ({ value: x.type, label: x.label })), f.type, (v) => {
          f.type = v; f.value = ""; renderBuilder();
        }),
        valueControl,
        h("button", { type: "button", class: "protein-row__del", onclick: () => { b.filters.splice(i, 1); renderBuilder(); } }, "×"),
      ));
    });

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
      const fieldInput = h("input", { class: "startup-field__input protein-input", type: "text",
        placeholder: s.dir === "topo" ? "link kind" : "field (quantity, slug)", value: s.field || "",
        list: s.dir === "topo" ? "protein-link-kinds" : null });
      fieldInput.addEventListener("input", () => { s.field = fieldInput.value; });
      sortsWrap.append(h("div", { class: "protein-row" },
        fieldInput,
        dropdown(SORT_DIRS, s.dir, (v) => { s.dir = v; renderBuilder(); }),
        h("button", { type: "button", class: "protein-row__del", onclick: () => { b.sorts.splice(i, 1); renderBuilder(); } }, "×"),
      ));
    });

    const limitInput = h("input", { class: "startup-field__input protein-input protein-input--sm", type: "number", placeholder: "no limit", value: b.limit });
    limitInput.addEventListener("input", () => { b.limit = limitInput.value; });

    builderEl.replaceChildren(
      field("Name", nameInput),
      field("Show", dropdown(SOURCES.map((s) => ({ value: s, label: s === "record" ? "records" : s === "promise" ? "promises" : "decisions" })), b.source, (v) => { b.source = v; })),
      h("div", { class: "protein-group" },
        h("div", { class: "protein-group__head" }, "Filters",
          h("button", { type: "button", class: "protein-add", onclick: () => { b.filters.push({ type: "kind_eq", value: "plain" }); renderBuilder(); } }, "+ filter")),
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
          h("button", { type: "button", class: "protein-add", onclick: () => { b.sorts.push({ field: "quantity", dir: "desc" }); renderBuilder(); } }, "+ sort")),
        sortsWrap),
      field("Limit", limitInput),
      h("div", { class: "protein-actions" },
        h("button", { type: "button", class: "button button--accent", onclick: () => void save() }, "Save"),
        h("button", { type: "button", class: "button button--ghost", onclick: () => renderList() }, "Cancel"),
      ),
      h("datalist", { id: "protein-link-kinds" }, ...conceptNames.map((n) => h("option", { value: n }))),
    );
  }

  async function save() {
    const name = String(builder.name || "").trim();
    if (!name) { setHelp("Give it a name.", true); return; }
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
      cardId = null;
      mode = "list";
    },
  };
}
