// Host state and subscriptions.
//
// This module owns everything that talks to the bridge and nothing that draws.
// The rule it enforces: the sand holds no durable truth. Records, entries,
// rules, occurrences and every total arrive from Protein; what lives here is
// the last snapshot and the filters a person picked, which are UI preferences
// and nothing more.

const HOST = window.LinceWidgetHost;

export const state = {
  records: [],
  concepts: [],
  conceptNames: new Map(),
  entries: [],
  rules: [],
  occurrences: [],
  timeline: null,
  live: false,
  notice: null,
  // Which entry is open for inline editing; UI only.
  editing: null,
  filters: {
    entriesConcept: "",
    graphConcept: "",
    graphMonths: 6,
  },
};

const listeners = new Set();

export function onChange(handler) {
  listeners.add(handler);
  return () => listeners.delete(handler);
}

export function notifyChanged() {
  for (const handler of listeners) handler();
}

export function setNotice(message, tone = "error") {
  state.notice = message ? { message, tone } : null;
  notifyChanged();
}

export function hasHost() {
  return typeof HOST?.subscribeProtein === "function" && typeof HOST?.act === "function";
}

// Actions that change what this sand shows WITHOUT committing a Fact.
//
// Live invalidation is driven by committed Facts, so these would otherwise
// leave the screen stale until some unrelated capture happened to land: a rule
// declares and moves nothing, pausing and skipping decide and move nothing, and
// re-tagging is an assertion about a Fact rather than a new one. Making any of
// them append a Fact to force a refresh would be the wrong fix — it would put a
// phantom change in the Ledger — so the surface re-reads instead.
const SILENT_ACTIONS = new Set([
  "create-recurrence",
  "revise-recurrence",
  "set-recurrence-paused",
  "skip-recurrence-occurrence",
  "unskip-recurrence-occurrence",
  "classify-fact",
]);

/**
 * Send a typed Action.
 *
 * Failures surface as a notice rather than being swallowed: a capture that
 * silently did nothing is worse than one that says why.
 */
export async function act(action) {
  if (typeof HOST?.act !== "function") {
    setNotice("This host cannot submit Actions.");
    return null;
  }
  try {
    const result = await HOST.act(action);
    const warnings = Array.isArray(result?.warnings) ? result.warnings : [];
    if (warnings.length) setNotice(warnings.join("; "), "ok");
    else setNotice(null);
    if (SILENT_ACTIONS.has(action?.action)) refreshSilent();
    return result;
  } catch (error) {
    setNotice(error?.message || "That action was refused.");
    return null;
  }
}

/** Re-open the reads a Factless change would not have invalidated. */
function refreshSilent() {
  subscribeRecurrence();
  subscribeEntries();
  subscribeTimeline();
}

function subscribe(subId, protein, apply) {
  if (typeof HOST?.subscribeProtein !== "function") return () => {};
  return HOST.subscribeProtein(subId, protein, (snapshot = {}) => {
    if (snapshot.error) {
      setNotice(snapshot.message || `The ${subId} query failed.`);
      return;
    }
    state.live = true;
    apply(Array.isArray(snapshot.rows) ? snapshot.rows : []);
    notifyChanged();
  });
}

let unsubscribeTimeline = null;
let unsubscribeEntries = null;
let unsubscribeRecurrence = null;

export function subscribeAll() {
  subscribe("records", { source: "record", limit: 500 }, (rows) => {
    // Only things that can actually hold a quantity are capture targets.
    state.records = rows.filter((row) => row?.uid);
  });

  subscribe("concepts", { source: "concept", limit: 1000 }, (rows) => {
    state.concepts = rows;
    state.conceptNames = new Map(
      rows.filter((r) => r?.uid).map((r) => [r.uid, r.name || r.canonical_name || r.uid]),
    );
  });

  subscribeRecurrence();
  subscribeEntries();
  subscribeTimeline();
}

export function subscribeRecurrence() {
  unsubscribeRecurrence?.();
  unsubscribeRecurrence = subscribe("recurrence", { source: "recurrence" }, (rows) => {
    state.rules = rows.filter((row) => row?.kind === "recurrence");
    state.occurrences = rows.filter((row) => row?.kind === "occurrence");
  });
}

/** Re-open the entry query when the category filter changes. */
export function subscribeEntries() {
  unsubscribeEntries?.();
  const filter = [];
  if (state.filters.entriesConcept) {
    filter.push({ classified_in: state.filters.entriesConcept });
  }
  unsubscribeEntries = subscribe(
    "entries",
    { source: "entry", where: filter, limit: 300 },
    (rows) => {
      state.entries = rows.filter((row) => row?.kind === "entry");
    },
  );
}

/**
 * Re-open the timeline when the charted concept or window changes.
 *
 * The window is sent as absolute instants rather than a trailing duration so
 * the past and future halves are symmetrical around now.
 */
export function subscribeTimeline() {
  unsubscribeTimeline?.();
  unsubscribeTimeline = null;
  const concept = state.filters.graphConcept;
  if (!concept) {
    state.timeline = null;
    notifyChanged();
    return;
  }
  const months = Number(state.filters.graphMonths) || 6;
  const now = new Date();
  const since = new Date(now);
  since.setMonth(since.getMonth() - months);
  const before = new Date(now);
  before.setMonth(before.getMonth() + months);

  unsubscribeTimeline = subscribe(
    "timeline",
    {
      source: "timeline",
      where: [
        { classified_in: concept },
        { at_since: since.toISOString() },
        { at_before: before.toISOString() },
      ],
    },
    (rows) => {
      const context = rows.find((row) => row?.kind === "timeline_context") || null;
      state.timeline = {
        context,
        points: rows.filter((row) => row?.kind === "timeline_point"),
        sources: rows.filter((row) => row?.kind === "timeline_source"),
      };
    },
  );
}

/** Resolve a typed `@name` or uid to whatever the Actions accept. */
export function conceptToken(raw) {
  const value = String(raw ?? "").trim().replace(/^@/, "");
  return value || null;
}
