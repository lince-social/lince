const ATTENTION = new Set(["broken", "partially_settled"]);
const OPEN = new Set(["draft", "proposed", "agreed", "in_transfer", "inactive", "broken", "partially_settled"]);
const SETTLED = new Set(["settled", "withdrawn"]);

const STATUS_ORDER = {
  broken: 0,
  partially_settled: 1,
  in_transfer: 2,
  proposed: 3,
  draft: 4,
  agreed: 5,
  inactive: 6,
  settled: 7,
  withdrawn: 8,
};

export function normalizeTransfer(raw) {
  const row = raw && typeof raw === "object" ? raw : {};
  const agreement = row.agreement && typeof row.agreement === "object" ? row.agreement : {};
  const progress = row.progress && typeof row.progress === "object" ? row.progress : {};
  const parties = Array.isArray(row.parties) ? row.parties : [];
  const promises = Array.isArray(row.promises) ? row.promises : [];
  const confirmations = Array.isArray(row.confirmations) ? row.confirmations : [];
  return {
    ...row,
    uid: String(row.uid || ""),
    slug: String(row.slug || ""),
    head: String(row.head || row.slug || row.uid || "Untitled transfer"),
    status: String(row.status || "draft"),
    visibility: String(row.visibility || "hidden"),
    agreement_type: String(row.agreement_type || "full"),
    agreement: {
      reviewed: number(agreement.reviewed),
      committed: number(agreement.committed),
      required: number(agreement.required),
      total: number(agreement.total || parties.length),
      policy_satisfied: Boolean(agreement.policy_satisfied),
    },
    progress,
    parties,
    promises,
    confirmations,
    balance: row.balance && typeof row.balance === "object" ? row.balance : {},
    balance_detail: Array.isArray(row.balance_detail) ? row.balance_detail : [],
    balanced: Boolean(row.balanced),
    active: Boolean(row.active),
    require_confirmation: Boolean(row.require_confirmation),
  };
}

export function transferMatches(row, filter, query) {
  if (filter === "attention" && !needsAttention(row)) return false;
  if (filter === "open" && !OPEN.has(row.status)) return false;
  if (filter === "settled" && !SETTLED.has(row.status)) return false;
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return true;
  return searchableText(row).includes(needle);
}

export function filterAndSort(rows, filter, query, sort) {
  const result = rows.filter((row) => transferMatches(row, filter, query));
  result.sort((left, right) => {
    if (sort === "name") return left.head.localeCompare(right.head);
    if (sort === "status") {
      return statusOrder(left) - statusOrder(right) || left.head.localeCompare(right.head);
    }
    return attentionOrder(left) - attentionOrder(right) || left.head.localeCompare(right.head);
  });
  return result;
}

export function summarize(rows) {
  return {
    total: rows.length,
    attention: rows.filter(needsAttention).length,
    open: rows.filter((row) => OPEN.has(row.status)).length,
    settled: rows.filter((row) => row.status === "settled").length,
  };
}

export function needsAttention(row) {
  if (ATTENTION.has(row.status)) return true;
  if (!row.active && row.promises.length > 0) return true;
  if (row.require_confirmation && row.status === "partially_settled") return true;
  return row.promises.some((promise) => promise.state === "broken");
}

export function statusLabel(status) {
  return String(status || "draft")
    .replaceAll("_", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

export function agreementLabel(row) {
  const type = row.agreement_type === "percentage"
    ? `${number(row.agreement_pct)}% threshold`
    : statusLabel(row.agreement_type);
  const count = row.agreement.required > 0
    ? `${row.agreement.committed}/${row.agreement.required} committed`
    : `${row.agreement.committed}/${row.agreement.total} committed`;
  return `${type} · ${count}`;
}

export function partyName(party) {
  return String(party?.actor_head || party?.actor_slug || party?.actor || "Unknown person");
}

export function promiseName(promise) {
  return String(promise?.record_head || promise?.record_slug || promise?.record || "Unbound promise");
}

export function formatQuantity(value, unit = "") {
  const quantity = Number(value);
  if (!Number.isFinite(quantity)) return "—";
  const formatted = new Intl.NumberFormat(undefined, { maximumFractionDigits: 3 }).format(quantity);
  return unit ? `${formatted} ${unit}` : formatted;
}

export function formatDate(value) {
  if (!value) return "No deadline";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return String(value);
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(parsed);
}

export function balanceEntries(row) {
  if (row.balance_detail.length) {
    return row.balance_detail
      .map((entry) => [entry.concept_name || entry.concept || "Unclassified", Number(entry.delta) || 0])
      .sort(([a], [b]) => a.localeCompare(b));
  }
  return Object.entries(row.balance)
    .map(([concept, value]) => [concept === "(none)" ? "Unclassified" : concept, Number(value) || 0])
    .sort(([a], [b]) => a.localeCompare(b));
}

export function progressLabel(progress) {
  const entries = Object.entries(progress || {}).filter(([, count]) => Number(count) > 0);
  if (!entries.length) return "No promises";
  return entries
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([state, count]) => `${count} ${statusLabel(state)}`)
    .join(" · ");
}

function searchableText(row) {
  const values = [row.head, row.slug, row.uid, row.status, row.visibility];
  for (const party of row.parties) values.push(partyName(party));
  for (const promise of row.promises) {
    values.push(promiseName(promise), promise.concept_name, promise.unit_name, promise.state);
  }
  return values.filter(Boolean).join(" ").toLocaleLowerCase();
}

function attentionOrder(row) {
  if (needsAttention(row)) return 0;
  if (OPEN.has(row.status)) return 1;
  return 2;
}

function statusOrder(row) {
  return STATUS_ORDER[row.status] ?? 99;
}

function number(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}
