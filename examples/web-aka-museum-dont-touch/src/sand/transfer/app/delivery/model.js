const DELIVERY_STATES = new Set([
  "not_queued",
  "queued",
  "sending",
  "delivered",
  "received",
  "seen",
  "failed",
  "revoked",
]);

const MODES = new Set(["hosted", "replicated"]);

export function normalizeSocialDelivery(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const authority = normalizedObject(raw.authority);
  const localView = normalizedObject(raw.local_view);
  const recipients = projectedArray(raw.recipients, "items").map(normalizeRecipient);
  const eligibleRecipients = projectedArray(
    raw.eligible_recipients ?? raw.recipient_options,
    "items",
    "recipients",
  ).map(normalizeRecipient);
  const receipts = projectedArray(raw.package_receipts ?? raw.receipts, "items", "events")
    .map(normalizeReceipt);
  const conflicts = projectedArray(raw.conflicts, "items").map(normalizeConflict);
  return {
    ...raw,
    authority,
    local_view: localView,
    authority_organ: nullableString(raw.authority_organ ?? authority.origin_organ),
    local_organ: nullableString(raw.local_organ ?? authority.local_organ),
    authority_role: nullableString(raw.authority_role ?? authority.role),
    canonical_writes: nullableString(raw.canonical_writes ?? authority.canonical_writes),
    mode: normalizedMode(raw.mode ?? localView.mode),
    state: nullableString(raw.state ?? localView.state),
    revision: optionalNumber(raw.revision ?? raw.delivery_revision),
    freshness: normalizeFreshness(raw.freshness ?? localView.freshness),
    recipients,
    eligible_recipients: eligibleRecipients,
    package_receipts: receipts,
    conflicts,
    replica_history: projectedArray(raw.replica_history ?? localView.replica_history, "items", "revisions"),
    // Which Cell retries this Transfer (Ontology C7). Normalised like every
    // other block so a host that has not shipped it yet renders the "any Cell"
    // state rather than throwing on an absent field.
    executor: normalizedObject(raw.executor),
    capabilities: normalizedObject(raw.capabilities),
    blocking_reasons: normalizedObject(raw.blocking_reasons),
    action_payloads: normalizedObject(raw.action_payloads ?? raw.actions),
  };
}

export function deliveryIndicator(delivery) {
  if (!delivery) return null;
  if (delivery.conflicts.length) return { state: "conflict", label: "Delivery conflict" };
  if (delivery.recipients.some((recipient) => recipient.state === "failed")) {
    return { state: "failed", label: "Delivery failed" };
  }
  if (delivery.freshness.state && delivery.freshness.state !== "fresh") {
    return { state: delivery.freshness.state, label: delivery.freshness.state };
  }
  const pending = delivery.recipients.filter((recipient) => ["queued", "sending"].includes(recipient.state)).length;
  if (pending) return { state: "queued", label: `${pending} queued` };
  if (delivery.authority_role === "replica") {
    return { state: "replica", label: delivery.mode || "Replica" };
  }
  if (delivery.authority_role === "hosted_reference") {
    return { state: "hosted", label: delivery.mode || "Hosted" };
  }
  return null;
}

export function projectedAction(scope, key, variant = null) {
  if (!scope || typeof scope !== "object") return null;
  const candidates = variant ? [
    scope.action_payloads?.[`${key}_${variant}`],
    scope.action_payloads?.[key]?.[variant],
    scope.actions?.[`${key}_${variant}`],
    scope.actions?.[key]?.[variant],
  ] : [];
  candidates.push(
    scope.action_payloads?.[key],
    scope.actions?.[key],
    actionFromCapability(scope.capabilities?.[key]),
  );
  const projected = candidates.find(isActionPayload);
  return projected ? clone(projected) : null;
}

export function capability(scope, ...keys) {
  return keys.some((key) => {
    const value = scope?.capabilities?.[key];
    return value === true || value?.allowed === true || value?.enabled === true;
  });
}

export function blockers(scope, ...keys) {
  const values = [];
  for (const key of keys) {
    const value = scope?.blocking_reasons?.[key];
    if (Array.isArray(value)) values.push(...value);
    else if (value != null && value !== "") values.push(value);
  }
  return values;
}

export function withActionInput(action, fields = {}) {
  if (!isActionPayload(action)) return null;
  const next = clone(action);
  for (const [key, value] of Object.entries(fields)) {
    if (Object.hasOwn(next, key)) next[key] = value;
  }
  if (next.request_id == null || next.request_id === "") next.request_id = requestId();
  return next;
}

export function recipientKey(recipient) {
  return String(recipient.uid || recipient.delivery_uid
    || `${recipient.person || "person?"}:${recipient.organ || "organ?"}`);
}

function normalizeRecipient(raw) {
  const row = raw && typeof raw === "object" ? raw : {};
  const delivery = normalizedObject(row.delivery);
  const mode = normalizedMode(row.mode);
  return {
    ...row,
    uid: nullableString(row.uid ?? row.delivery_uid),
    delivery_uid: nullableString(row.delivery_uid ?? delivery.uid ?? row.uid),
    person: nullableString(row.person ?? row.person_uid ?? row.recipient_person),
    person_head: nullableString(row.person_head ?? row.recipient_person_head),
    person_slug: nullableString(row.person_slug ?? row.recipient_person_slug),
    organ: nullableString(row.organ ?? row.organ_uid ?? row.recipient_organ),
    organ_head: nullableString(row.organ_head ?? row.recipient_organ_head),
    organ_slug: nullableString(row.organ_slug ?? row.recipient_organ_slug),
    contact_state: nullableString(row.contact_state ?? row.trust),
    mode,
    default_mode: normalizedMode(row.default_mode),
    state: normalizeDeliveryState(delivery.status ?? row.delivery_status ?? row.state),
    policy_state: nullableString(row.state),
    revision: optionalNumber(row.revision ?? row.delivery_revision),
    attempts: optionalNumber(row.attempts ?? delivery.attempts),
    last_attempt_at: row.last_attempt_at ?? delivery.last_attempt_at ?? null,
    last_error: row.last_error ?? delivery.last_error ?? null,
    acknowledged_at: row.acknowledged_at ?? delivery.acknowledged_at ?? null,
    seen_at: row.seen_at ?? delivery.seen_at ?? null,
    granted_at: row.granted_at ?? null,
    revoked_at: row.revoked_at ?? null,
    delivery,
    freshness: normalizeFreshness(row.freshness),
    capabilities: normalizedObject(row.capabilities),
    blocking_reasons: normalizedObject(row.blocking_reasons),
    action_payloads: normalizedObject(row.action_payloads ?? row.actions),
    available_modes: Array.isArray(row.available_modes)
      ? row.available_modes.map(normalizedMode).filter(Boolean) : [],
  };
}

function normalizeFreshness(raw) {
  const row = raw && typeof raw === "object" ? raw : {};
  return {
    ...row,
    state: nullableString(row.state ?? row.status),
    projected_at: row.projected_at ?? row.refreshed_at ?? row.at ?? null,
    last_pull_at: row.last_pull_at ?? null,
    remote_revision: optionalNumber(row.remote_revision),
    local_revision: optionalNumber(row.local_revision),
    cursor: optionalNumber(row.cursor ?? row.accepted_cursor),
    origin_cursor: optionalNumber(row.origin_cursor),
    last_error: nullableString(row.last_error),
  };
}

function normalizeReceipt(raw) {
  const row = raw && typeof raw === "object" ? raw : {};
  return {
    ...row,
    uid: nullableString(row.uid),
    kind: nullableString(row.kind ?? row.event ?? row.state),
    envelope: nullableString(row.envelope ?? row.envelope_uid),
    cursor: optionalNumber(row.cursor ?? row.origin_cursor),
    organ: nullableString(row.organ ?? row.organ_uid ?? row.actor_organ),
    at: row.at ?? row.created_at ?? null,
  };
}

function normalizeConflict(raw) {
  const row = raw && typeof raw === "object" ? raw : {};
  return {
    ...row,
    uid: nullableString(row.uid ?? row.attempt_uid ?? row.request_id),
    state: nullableString(row.state ?? row.kind) || "conflict",
    request_id: nullableString(row.request_id ?? row.idempotency_key),
    at: row.at ?? row.created_at ?? null,
    capabilities: normalizedObject(row.capabilities),
    blocking_reasons: normalizedObject(row.blocking_reasons),
    action_payloads: normalizedObject(row.action_payloads ?? row.actions),
  };
}

function projectedArray(value, ...keys) {
  if (Array.isArray(value)) return value;
  for (const key of keys) if (Array.isArray(value?.[key])) return value[key];
  return [];
}

function actionFromCapability(value) {
  if (!value || typeof value !== "object") return null;
  return value.action_payload ?? value.payload
    ?? (typeof value.action === "string" ? value : null);
}

function isActionPayload(value) {
  return Boolean(value && typeof value === "object" && !Array.isArray(value)
    && typeof value.action === "string" && value.action);
}

function normalizedMode(value) {
  const mode = String(value || "");
  return MODES.has(mode) ? mode : null;
}

function normalizeDeliveryState(value) {
  const state = String(value || "");
  return DELIVERY_STATES.has(state) ? state : nullableString(value);
}

function normalizedObject(value) {
  return value && typeof value === "object" && !Array.isArray(value) ? value : {};
}

function nullableString(value) {
  return value == null || value === "" ? null : String(value);
}

function optionalNumber(value) {
  if (value == null || value === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function requestId() {
  if (globalThis.crypto?.randomUUID) return `transfer-delivery:${globalThis.crypto.randomUUID()}`;
  return `transfer-delivery:${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
