import { normalizeSocialDelivery } from "./delivery/model.js";

const EXCEPTION_PRIMARY_STATUSES = new Set([
  "system_disputed",
  "disputed",
  "participant_disputed",
  "broken",
  "expired",
  "partially_settled",
]);
const STATUS_ORDER = {
  system_disputed: 0,
  disputed: 1,
  participant_disputed: 1,
  broken: 2,
  expired: 3,
  cancelled: 4,
  partially_settled: 5,
  awaiting_me: 6,
  active: 7,
  in_transfer: 7,
  awaiting_others: 8,
  agreed: 9,
  proposed: 10,
  open: 11,
  discoverable_open: 11,
  completed: 12,
  settled: 12,
  satiated: 12,
  inactive: 13,
  draft: 14,
  legacy: 15,
  withdrawn: 16,
};

export const WORKFLOW_FACETS = [
  "awaiting_me",
  "awaiting_others",
  "active",
  "completed",
  "cancelled_or_broken",
  "discoverable_open",
];

export function normalizeTransfer(raw) {
  const row = raw && typeof raw === "object" ? raw : {};
  const signedSnapshot = row.revision_evidence?.current?.terms;
  const signedTransfer = signedSnapshot?.transfer && typeof signedSnapshot.transfer === "object" ? signedSnapshot.transfer : {};
  const agreement = row.agreement && typeof row.agreement === "object" ? row.agreement : {};
  const readinessProjection = row.readiness ?? row.agreement_readiness ?? row.activation_readiness ?? null;
  const progress = row.progress && typeof row.progress === "object" ? row.progress : {};
  const projectedParties = Array.isArray(row.parties) ? row.parties : [];
  const projectedInvitations = Array.isArray(row.invitations) ? row.invitations : [];
  const projectedPromises = Array.isArray(row.promises) ? row.promises : [];
  const projectedOccurrences = Array.isArray(row.occurrences) ? row.occurrences : [];
  const parties = Array.isArray(signedSnapshot?.parties)
    ? signedSnapshot.parties.map((signed) => {
      const projected = projectedParties.find((party) => party.uid === signed.uid || party.actor === signed.person_uid) || {};
      return {
        ...signed,
        ...projected,
        actor: projected.actor || signed.person_uid,
      };
    })
    : projectedParties;
  const invitations = Array.isArray(signedSnapshot?.invitations)
    ? signedSnapshot.invitations.map((signed) => {
      const projected = projectedInvitations.find((invitation) => invitation.uid === signed.uid) || {};
      return {
        ...signed,
        ...projected,
        addressed_person: projected.addressed_person || projected.addressed_person_uid || signed.addressed_person_uid,
      };
    })
    : projectedInvitations;
  const promises = Array.isArray(signedSnapshot?.promises)
    ? signedSnapshot.promises.map((signed) => {
      const projected = projectedPromises.find((promise) => promise.uid === signed.uid) || {};
      return {
      ...signed,
      ...projected,
      record: signed.record_uid,
      party: signed.person_uid,
      unit: signed.unit_uid,
      place: signed.location,
      reuse_policy: signed.open_reuse_policy,
      source_promise: signed.source_promise_uid || signed.source_promise,
      state: projected.state || signed.state,
      open: Boolean(signed.open || signed.state === "open" || projected.open || projected.state === "open"),
      proposer: projected.proposer || projected.proposer_person || projected.proposer_person_uid
        || (signed.state === "open" ? signed.person_uid : null),
      proposer_head: projected.proposer_head,
      proposer_slug: projected.proposer_slug,
      claim_pairs: projected.claim_pairs || signed.claim_pairs,
    };
    })
    : projectedPromises;
  const confirmations = Array.isArray(row.confirmations) ? row.confirmations : [];
  const normalizedParties = parties.map((party) => normalizeParty(
    party,
    readinessProjection?.people?.[party.actor || party.person_uid],
  ));
  return {
    ...row,
    uid: String(row.uid || ""),
    slug: String(signedTransfer.slug || row.slug || ""),
    head: String(signedTransfer.head || row.head || row.slug || row.uid || "Untitled transfer"),
    status: String(row.status === "disputed" && row.correction_status === "participant_disputed"
      ? "participant_disputed" : row.status || "draft"),
    primary_status: row.primary_status || row.operational_status
      ? String(row.primary_status || row.operational_status) : null,
    operational_status: row.operational_status || row.primary_status
      ? String(row.operational_status || row.primary_status) : null,
    inbox_facets: normalizeInboxFacets(row.inbox_facets),
    inbox_projection_complete: hasCompleteInboxProjection(row),
    viewer_roles: Array.isArray(row.viewer_roles) ? row.viewer_roles.map(String) : [],
    revision: number(row.revision),
    visibility: String(signedTransfer.visibility || row.visibility || "hidden"),
    agreement_type: String(signedTransfer.agreement_type || row.agreement_type || "full"),
    agreement: {
      reviewed: number(agreement.reviewed),
      committed: number(agreement.committed),
      required: number(agreement.required),
      total: number(agreement.total || normalizedParties.length),
      policy_satisfied: Boolean(agreement.policy_satisfied),
      coalition: normalizeCoalition(agreement.coalition || row.coalition),
      coalition_revision: number(agreement.coalition?.revision || agreement.coalition_revision || row.coalition?.revision || row.coalition_revision),
      readiness: normalizeReadiness(agreement.readiness ?? row.agreement_readiness ?? {
        ready: agreement.ready,
        blockers: agreement.blockers,
      }),
      events: normalizeAgreementEvents(agreement.events || agreement.history || row.agreement_events || row.agreement_history),
    },
    progress,
    settlement_progress: row.settlement_progress && typeof row.settlement_progress === "object"
      ? row.settlement_progress : {},
    hierarchy: normalizeHierarchy(row.hierarchy, row),
    phase6: normalizePhase6(row.phase6),
    first_completes: row.first_completes && typeof row.first_completes === "object"
      ? row.first_completes : null,
    parties: normalizedParties,
    invitations: invitations.map(normalizeInvitation),
    promises: promises.map((promise) => normalizePromise(promise, readinessProjection?.promises?.[promise.uid])),
    occurrences: projectedOccurrences.map(normalizeOccurrence),
    dependencies: normalizeDependencies(signedSnapshot?.dependencies || signedTransfer.dependencies || row.dependencies, row.dependency_status || row.dependencies),
    viewer_party: normalizeViewerParty(row.viewer_party, normalizedParties),
    readiness: normalizeReadiness(row.readiness ?? row.transfer_readiness ?? row.activation_readiness ?? {
      ready: agreement.ready ?? row.ready,
      blockers: agreement.blockers || row.readiness_blockers,
    }),
    threads: Array.isArray(row.threads) ? row.threads.map(normalizeThread) : row.threads,
    social_delivery: normalizeSocialDelivery(row.social_delivery),
    confirmations,
    capabilities: row.capabilities && typeof row.capabilities === "object" ? row.capabilities : {},
    blocking_reasons: row.blocking_reasons && typeof row.blocking_reasons === "object" ? row.blocking_reasons : {},
    balance: row.balance && typeof row.balance === "object" ? row.balance : {},
    balance_detail: Array.isArray(row.balance_detail) ? row.balance_detail : [],
    balanced: Boolean(row.balanced),
    active: Boolean(row.active),
    agreement_pct: signedTransfer.agreement_pct ?? row.agreement_pct,
    max_proximity: signedTransfer.max_proximity ?? row.max_proximity,
    satiation: signedTransfer.satiation ?? row.satiation,
    parent: signedTransfer.parent_uid ?? row.parent,
    source: signedTransfer.source_uid ?? row.source,
    reserve_default: signedTransfer.reserve_default ?? row.reserve_default,
    default_place: signedTransfer.default_place ?? row.default_place ?? null,
    require_confirmation: Boolean(signedTransfer.require_confirmation ?? row.require_confirmation),
  };
}

function normalizeInboxFacets(raw) {
  const value = raw && typeof raw === "object" ? raw : {};
  return Object.fromEntries([
    "mine",
    "invited",
    ...WORKFLOW_FACETS,
  ].map((key) => [key, typeof value[key] === "boolean" ? value[key] : null]));
}

function hasCompleteInboxProjection(row) {
  const facets = row?.inbox_facets;
  return Boolean((row?.primary_status || row?.operational_status)
    && facets && typeof facets === "object"
    && ["mine", "invited", ...WORKFLOW_FACETS]
      .every((key) => typeof facets[key] === "boolean"));
}

function normalizeHierarchy(hierarchy, transfer) {
  const row = hierarchy && typeof hierarchy === "object" ? hierarchy : {};
  const path = Array.isArray(row.path) ? row.path.map(String).filter(Boolean) : [];
  const children = Array.isArray(row.children) ? row.children.map(String).filter(Boolean) : [];
  return {
    ...row,
    root: row.root || path[0] || transfer.uid || null,
    parent: row.parent ?? transfer.parent ?? null,
    children,
    depth: number(row.depth),
    path,
    leaf: Boolean(row.leaf ?? children.length === 0),
    rollup: row.rollup && typeof row.rollup === "object" ? row.rollup : {},
    branches: Array.isArray(row.branches) ? row.branches : [],
  };
}

function normalizePhase6(phase6) {
  const row = phase6 && typeof phase6 === "object" ? phase6 : {};
  return {
    ...row,
    dependency_order: Array.isArray(row.dependency_order) ? row.dependency_order.map(String) : [],
    dependency_cycle: Boolean(row.dependency_cycle),
    hierarchy_cycle: Boolean(row.hierarchy_cycle),
    ready: typeof row.ready === "boolean" ? row.ready : null,
    blockers: Array.isArray(row.blockers) ? row.blockers : [],
  };
}

function normalizeInvitation(invitation) {
  const row = invitation && typeof invitation === "object" ? invitation : {};
  return {
    ...row,
    uid: String(row.uid || ""),
    addressed_person: String(row.addressed_person || row.addressed_person_uid || ""),
    status: String(row.status || "pending"),
    attempt_number: number(row.attempt_number || row.attempt || 1),
    expires_at: row.expires_at || row.expiry || null,
    events: Array.isArray(row.events) ? row.events : Array.isArray(row.lifecycle_events) ? row.lifecycle_events : [],
    attempts: Array.isArray(row.attempts) ? row.attempts : [],
    capabilities: row.capabilities && typeof row.capabilities === "object" ? row.capabilities : {},
  };
}

function normalizeParty(party, projectedReadiness) {
  const row = party && typeof party === "object" ? party : {};
  return {
    ...row,
    actor: String(row.actor || row.person || row.person_uid || ""),
    level: number(row.level ?? row.agreement_level),
    agreement_events: normalizeAgreementEvents(row.agreement_events || row.events || row.level_history),
    capabilities: row.capabilities && typeof row.capabilities === "object" ? row.capabilities : {},
    blocking_reasons: row.blocking_reasons && typeof row.blocking_reasons === "object" ? row.blocking_reasons : {},
    readiness: normalizeReadiness(row.readiness ?? row.agreement_readiness ?? row.activation_readiness ?? projectedReadiness ?? {
      ready: row.ready,
      blockers: row.readiness_blockers,
    }),
    in_coalition: Boolean(row.in_coalition || row.coalition_member),
  };
}

function normalizePromise(promise, projectedReadiness) {
  const row = promise && typeof promise === "object" ? promise : {};
  const open = Boolean(row.open || row.state === "open");
  const party = row.party || row.person_uid || null;
  return {
    ...row,
    uid: String(row.uid || ""),
    party,
    open,
    proposer: row.proposer || row.proposer_person || row.proposer_person_uid || (open ? party : null),
    claim_pairs: (Array.isArray(row.claim_pairs) ? row.claim_pairs
      : Array.isArray(row.open_claim_pairs) ? row.open_claim_pairs : []).map(normalizeOpenClaimPair),
    source_promise: row.source_promise || row.source_promise_uid || null,
    capabilities: row.capabilities && typeof row.capabilities === "object" ? row.capabilities : {},
    blocking_reasons: row.blocking_reasons && typeof row.blocking_reasons === "object" ? row.blocking_reasons : {},
    availability: normalizeAvailability(row.availability),
    readiness: normalizeReadiness(row.readiness ?? row.agreement_readiness ?? projectedReadiness ?? {
      ready: row.agreement_ready,
      eligible: row.agreement_eligible,
      blockers: row.agreement_blockers,
    }),
  };
}

function normalizeOpenClaimPair(pair) {
  const row = pair && typeof pair === "object" ? pair : {};
  return {
    ...row,
    uid: String(row.uid || ""),
    source: row.source || row.source_promise || row.source_promise_uid || null,
    proposer_promise: row.proposer_promise || row.proposer_promise_uid || null,
    claimant_promise: row.claimant_promise || row.claimant_promise_uid || null,
    proposer: row.proposer || row.proposer_person || row.proposer_person_uid || null,
    claimant: row.claimant || row.claimant_person || row.claimant_person_uid || null,
    revision: number(row.revision),
    reuse_policy: row.reuse_policy == null ? null : String(row.reuse_policy),
    request_id: row.request_id || row.idempotency_key || null,
    at: row.at || row.created_at || null,
  };
}

function normalizeOccurrence(occurrence) {
  const row = occurrence && typeof occurrence === "object" ? occurrence : {};
  const conclusion = row.conclusion && typeof row.conclusion === "object" ? row.conclusion : null;
  const activation = row.activation && typeof row.activation === "object" ? row.activation : null;
  const application = row.application && typeof row.application === "object" ? row.application : null;
  const settlementProgress = normalizeSettlementProgress(
    row.settlement_progress
      ?? (hasSettlementProgress(row.progress) ? row.progress : null),
    row.slices || row.settlement_slices,
  );
  const formulaProjected = Object.hasOwn(row, "application_formula")
    || (row.application && typeof row.application === "object" && Object.hasOwn(row.application, "formula"));
  const dispute = normalizeParticipantDispute(row.dispute, row.disputed);
  const systemDispute = normalizeSystemDispute(row.system_dispute, row.system_disputed);
  return {
    ...row,
    uid: String(row.uid || ""),
    path: String(row.path || row.exchange_path || ""),
    promise: row.promise || row.promise_uid || null,
    opposite_promise: row.opposite_promise || row.opposite_promise_uid || null,
    revision: number(row.revision),
    state: String(row.status || row.state || "active"),
    subject: row.record || row.subject || row.record_uid || row.subject_uid || null,
    subject_head: row.record_head || row.subject_head || null,
    subject_slug: row.record_slug || row.subject_slug || null,
    unit: row.unit || row.unit_uid || null,
    quantity: number(row.quantity),
    giver: row.giver || row.giver_person || row.giver_person_uid || null,
    receiver: row.receiver || row.receiver_person || row.receiver_person_uid || null,
    place: row.place ?? row.location ?? null,
    activation,
    availability: normalizeAvailability(row.availability),
    reservation: row.reservation && typeof row.reservation === "object" ? row.reservation : null,
    delivery: normalizeClaim(row.delivery || row.delivery_claim, row.delivery_claimed, row.delivery_history),
    receipt: normalizeClaim(row.receipt || row.receipt_claim, row.receipt_claimed, row.receipt_history),
    disputed: dispute.participant_disputed,
    participant_disputed: dispute.participant_disputed,
    dispute,
    system_disputed: systemDispute.disputed,
    system_dispute: systemDispute,
    conclusion,
    confirmed_conclusion: Boolean(row.confirmed_conclusion || row.concluded || row.conclusion === "confirmed_conclusion" || conclusion?.confirmed || conclusion?.complete),
    has_application_formula: formulaProjected,
    application_formula: row.application_formula ?? row.application?.formula ?? null,
    application,
    settlement_progress: settlementProgress,
    settlement_preview: normalizeSettlementPreview(row.settlement_preview),
    capabilities: row.capabilities && typeof row.capabilities === "object" ? row.capabilities : {},
    blocking_reasons: row.blocking_reasons && typeof row.blocking_reasons === "object" ? row.blocking_reasons : {},
  };
}

function hasSettlementProgress(value) {
  return value && typeof value === "object" && [
    "canonical_quantity",
    "settled_quantity",
    "remaining_quantity",
    "partially_settled",
    "settled",
    "slices",
  ].some((key) => Object.hasOwn(value, key));
}

function normalizeSettlementProgress(progress, projectedSlices) {
  const row = progress && typeof progress === "object" ? progress : null;
  const rawSlices = Array.isArray(row?.slices) ? row.slices
    : Array.isArray(projectedSlices) ? projectedSlices : [];
  if (!row && !rawSlices.length) return null;
  return {
    ...(row || {}),
    canonical_quantity: optionalNumber(row?.canonical_quantity),
    settled_quantity: optionalNumber(row?.settled_quantity),
    remaining_quantity: optionalNumber(row?.remaining_quantity),
    partially_settled: Boolean(row?.partially_settled),
    settled: Boolean(row?.settled),
    slices: rawSlices.map(normalizeSettlementSlice),
  };
}

export function normalizeSettlementPreview(preview) {
  if (!preview || typeof preview !== "object") return null;
  return {
    ...preview,
    canonical_quantity: optionalNumber(preview.canonical_quantity),
    remaining_quantity: optionalNumber(preview.remaining_quantity),
    remaining_after: optionalNumber(preview.remaining_after),
    settled_quantity: optionalNumber(preview.settled_quantity),
    local_delta: optionalNumber(preview.local_delta),
    local_cumulative_after: optionalNumber(preview.local_cumulative_after),
    owner: preview.owner || preview.person || preview.owner_person || preview.owner_person_uid || null,
    application_formula_hash: preview.application_formula_hash == null
      ? null : String(preview.application_formula_hash),
    application_formula_version: optionalNumber(preview.application_formula_version),
    remainder_policy: preview.remainder_policy == null ? null : String(preview.remainder_policy),
    local_record: preview.local_record || preview.local_record_uid || null,
    expected_remaining_quantity: optionalNumber(
      preview.expected_remaining_quantity ?? preview.remaining_quantity,
    ),
    expected_local_delta: optionalNumber(preview.expected_local_delta ?? preview.local_delta),
    expected_application_formula_hash: (preview.expected_application_formula_hash
      ?? preview.application_formula_hash) == null
      ? null : String(preview.expected_application_formula_hash ?? preview.application_formula_hash),
    expected_application_formula_version: optionalNumber(
      preview.expected_application_formula_version ?? preview.application_formula_version,
    ),
    expected_remainder_policy: (preview.expected_remainder_policy
      ?? preview.remainder_policy) == null
      ? null : String(preview.expected_remainder_policy ?? preview.remainder_policy),
  };
}

function normalizeSettlementSlice(slice) {
  const row = slice && typeof slice === "object" ? slice : {};
  const compensation = normalizeSettlementCompensation(row.compensation);
  return {
    ...row,
    uid: String(row.uid || row.settlement_uid || ""),
    owner: row.owner || row.owner_person || row.owner_person_uid || null,
    canonical_quantity: optionalNumber(row.canonical_quantity),
    cumulative_before: optionalNumber(row.cumulative_before),
    cumulative_after: optionalNumber(row.cumulative_after),
    remaining_after: optionalNumber(row.remaining_after),
    evidence_fact: row.evidence_fact || row.evidence_fact_uid || null,
    local_record: row.local_record || row.local_record_uid || null,
    local_delta: optionalNumber(row.local_delta),
    local_cumulative_before: optionalNumber(row.local_cumulative_before),
    local_cumulative_after: optionalNumber(row.local_cumulative_after),
    application_fact: row.application_fact || row.application_fact_uid || null,
    application_formula_hash: row.application_formula_hash == null
      ? null : String(row.application_formula_hash),
    application_formula_version: optionalNumber(row.application_formula_version),
    remainder_policy: row.remainder_policy == null ? null : String(row.remainder_policy),
    compensation_status: row.compensation_status == null ? null : String(row.compensation_status),
    compensation,
    capabilities: row.capabilities && typeof row.capabilities === "object" ? row.capabilities : {},
    blocking_reasons: row.blocking_reasons && typeof row.blocking_reasons === "object" ? row.blocking_reasons : {},
    at: row.at || row.created_at || null,
  };
}

function normalizeSettlementCompensation(compensation) {
  if (!compensation || typeof compensation !== "object") return null;
  return {
    uid: compensation.uid == null ? null : String(compensation.uid),
    fact: compensation.fact == null ? null : String(compensation.fact),
    original_application_fact: compensation.original_application_fact == null
      ? null : String(compensation.original_application_fact),
    local_record: compensation.local_record == null ? null : String(compensation.local_record),
    inverse_delta: optionalNumber(compensation.inverse_delta),
    request_id: compensation.request_id == null ? null : String(compensation.request_id),
    at: compensation.at || null,
  };
}

function normalizeParticipantDispute(dispute, projectedDisputed) {
  const row = dispute && typeof dispute === "object" ? dispute : {};
  const history = (Array.isArray(row.history) ? row.history : []).map((event) => ({
    uid: event?.uid == null ? null : String(event.uid),
    disputed: Boolean(event?.disputed),
    person: event?.person == null ? null : String(event.person),
    fact: event?.fact == null ? null : String(event.fact),
    request_id: event?.request_id == null ? null : String(event.request_id),
    at: event?.at || null,
  }));
  return {
    participant_disputed: Boolean(row.participant_disputed ?? projectedDisputed),
    history,
  };
}

function normalizeSystemDispute(dispute, projectedDisputed) {
  const row = dispute && typeof dispute === "object" ? dispute : {};
  return {
    disputed: Boolean(row.disputed ?? projectedDisputed),
    fact: row.fact == null ? null : String(row.fact),
    at: row.at || null,
  };
}

function normalizeClaim(claim, projectedClaimed, projectedHistory) {
  const row = claim && typeof claim === "object" ? claim : {};
  const history = Array.isArray(row.history) ? row.history
    : Array.isArray(row.events) ? row.events
      : Array.isArray(projectedHistory) ? projectedHistory : [];
  const latest = history[history.length - 1] || {};
  return {
    ...row,
    claimed: Boolean(row.claimed ?? row.current ?? row.confirmed ?? projectedClaimed),
    person: row.person || row.actor || row.person_uid || latest.person || latest.actor || null,
    fact: row.fact || row.fact_uid || latest.fact || latest.fact_uid || null,
    at: row.at || row.created_at || latest.at || latest.created_at || null,
    history,
  };
}

function normalizeAvailability(raw) {
  if (!raw || typeof raw !== "object") return null;
  const known = ["actual", "available", "reserved", "planned", "surplus", "unknown_units", "availability_unknown_units"]
    .some((key) => Object.hasOwn(raw, key));
  return known ? raw : null;
}

function normalizeViewerParty(raw, parties) {
  if (raw && typeof raw === "object") {
    const rawActor = String(raw.actor || raw.person || raw.person_uid || "");
    const matched = parties.find((party) => party.uid === raw.uid || (rawActor && party.actor === rawActor)) || {};
    const actor = rawActor || matched.actor || "";
    return { ...matched, ...raw, actor, level: number(raw.level ?? raw.agreement_level ?? matched.level) };
  }
  if (typeof raw === "string") return parties.find((party) => party.uid === raw || party.actor === raw) || null;
  return null;
}

function normalizeCoalition(raw) {
  const members = Array.isArray(raw)
    ? raw
    : Array.isArray(raw?.party_uids) ? raw.party_uids
      : Array.isArray(raw?.members) ? raw.members
        : Array.isArray(raw?.people) ? raw.people : [];
  return members.map((member) => String(member?.person || member?.actor || member?.person_uid || member?.party_uid || member?.uid || member)).filter(Boolean);
}

function normalizeAgreementEvents(raw) {
  return (Array.isArray(raw) ? raw : []).map((event) => ({
    ...event,
    person: String(event?.person || event?.actor_person || event?.person_uid || event?.party_uid || event?.actor || ""),
    from_level: number(event?.from_level ?? event?.from ?? 0),
    level: number(event?.to_level ?? event?.level),
    revision: number(event?.revision),
    at: event?.at || event?.created_at || null,
  }));
}

function normalizeReadiness(raw) {
  if (typeof raw === "boolean") return { ready: raw, blockers: [], known: true };
  const value = raw && typeof raw === "object" ? raw : {};
  const known = ["ready", "satisfied", "unlocked", "blockers", "blocking_reasons", "blocked_by", "eligible"]
    .some((key) => Object.hasOwn(value, key));
  return {
    ...value,
    ready: Boolean(value.ready ?? value.satisfied ?? value.unlocked),
    blockers: Array.isArray(value.blockers) ? value.blockers
      : Array.isArray(value.blocking_reasons) ? value.blocking_reasons
        : Array.isArray(value.blocked_by) ? value.blocked_by : [],
    known,
  };
}

function normalizeDependencies(raw, status) {
  const projected = Array.isArray(status) ? status : [];
  return (Array.isArray(raw) ? raw : []).map((dependency) => {
    const live = projected.find((candidate) => candidate?.uid === dependency?.uid) || {};
    return {
      ...dependency,
      ...live,
      uid: String(dependency?.uid || live?.uid || ""),
      scope: String(dependency?.scope || live?.scope || (dependency?.promise || dependency?.promise_uid ? "promise" : "transfer")),
      promise: dependency?.promise || dependency?.promise_uid || live?.promise || null,
      upstream_kind: String(dependency?.upstream_kind || live?.upstream_kind || "transfer"),
      upstream: String(dependency?.upstream || dependency?.upstream_uid || live?.upstream || ""),
      required_state: String(dependency?.required_state || live?.required_state || "kept"),
      satisfied: Boolean(live?.satisfied),
    };
  });
}

function normalizeThread(thread) {
  const row = thread && typeof thread === "object" ? thread : {};
  return {
    ...row,
    uid: String(row.uid || ""),
    head: String(row.head || "General"),
    messages: Array.isArray(row.messages) ? row.messages.map((message) => ({
      ...message,
      uid: String(message?.uid || ""),
      body: String(message?.body || ""),
      references: Array.isArray(message?.references) ? message.references.map((reference) => ({
        ...reference,
        uid: String(reference?.uid || ""),
        head: String(reference?.head || reference?.slug || reference?.uid || "Record"),
      })) : [],
    })) : [],
  };
}

export function transferMatches(row, filters, query, viewer = null) {
  const selected = normalizeInboxFilter(filters);
  if (selected.ownership === "mine" && !inboxFacet(row, "mine", viewer)) return false;
  if (selected.workflow !== "all" && !inboxFacet(row, selected.workflow, viewer)) return false;
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return true;
  return searchableText(row).includes(needle);
}

export function filterAndSort(rows, filters, query, sort, viewer = null) {
  const result = rows.filter((row) => transferMatches(row, filters, query, viewer));
  result.sort((left, right) => {
    if (sort === "name") return left.head.localeCompare(right.head);
    if (sort === "status") {
      return statusOrder(left) - statusOrder(right) || left.head.localeCompare(right.head);
    }
    return attentionOrder(left) - attentionOrder(right) || left.head.localeCompare(right.head);
  });
  return result;
}

export function summarize(rows, viewer = null) {
  return {
    total: rows.length,
    awaiting_me: rows.filter((row) => inboxFacet(row, "awaiting_me", viewer)).length,
    active: rows.filter((row) => inboxFacet(row, "active", viewer)).length,
    completed: rows.filter((row) => inboxFacet(row, "completed", viewer)).length,
  };
}

export function inboxCounts(rows, ownership, viewer = null) {
  const scoped = ownership === "mine"
    ? rows.filter((row) => inboxFacet(row, "mine", viewer))
    : rows;
  return {
    ownership: {
      all: rows.length,
      mine: rows.filter((row) => inboxFacet(row, "mine", viewer)).length,
    },
    workflow: Object.fromEntries([
      ["all", scoped.length],
      ...WORKFLOW_FACETS.map((facet) => [
        facet,
        scoped.filter((row) => inboxFacet(row, facet, viewer)).length,
      ]),
    ]),
  };
}

export function inboxFacet(row, facet, viewer = null) {
  void viewer;
  return row?.inbox_facets?.[facet] === true;
}

export function primaryStatus(row, viewer = null) {
  void viewer;
  if (row?.primary_status) return row.primary_status;
  if (row?.operational_status) return row.operational_status;
  return row?.status || "draft";
}

export function needsAttention(row) {
  return row?.inbox_facets?.awaiting_me === true
    || EXCEPTION_PRIMARY_STATUSES.has(primaryStatus(row));
}

export function statusLabel(status) {
  if (status === "system_disputed") return "System safety hold";
  if (status === "participant_disputed") return "Participant dispute";
  return String(status || "draft")
    .replaceAll("_", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

export function agreementLabel(row) {
  const type = row.agreement_type === "percentage"
    ? `${number(row.agreement_pct)}% threshold`
    : statusLabel(row.agreement_type);
  const count = row.agreement.required > 0
    ? `${row.agreement.committed}/${row.agreement.required} agreed`
    : `${row.agreement.committed}/${row.agreement.total} agreed`;
  return `${type} · ${count}`;
}

export function agreementMilestoneLabel(level) {
  if (Number(level) >= 2) return "Agreed";
  if (Number(level) >= 1) return "Checked · ready to agree";
  return "No agreement";
}

export function partyName(party) {
  return String(party?.actor_head || party?.actor_slug || party?.actor || "Unknown person");
}

export function invitationName(invitation) {
  return String(
    invitation?.addressed_person_head
      || invitation?.addressed_person_slug
      || invitation?.addressed_person
      || "Unknown person",
  );
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
  const values = [row.head, row.slug, row.uid, primaryStatus(row), row.visibility];
  for (const party of row.parties) values.push(partyName(party));
  for (const invitation of row.invitations) values.push(invitationName(invitation), invitation.status);
  for (const promise of row.promises) {
    values.push(promiseName(promise), promise.concept_name, promise.unit_name, promise.state);
  }
  for (const occurrence of row.occurrences) {
    values.push(occurrence.uid, occurrence.path, occurrence.subject, occurrence.giver, occurrence.receiver);
  }
  for (const thread of Array.isArray(row.threads) ? row.threads : []) {
    values.push(thread.head);
    for (const message of thread.messages) values.push(message.body, message.sender);
  }
  return values.filter(Boolean).join(" ").toLocaleLowerCase();
}

function attentionOrder(row) {
  if (inboxFacet(row, "awaiting_me")) return 0;
  if (inboxFacet(row, "awaiting_others")) return 1;
  return statusOrder(row) + 2;
}

function statusOrder(row) {
  return STATUS_ORDER[primaryStatus(row)] ?? 99;
}

function normalizeInboxFilter(raw) {
  if (raw && typeof raw === "object") {
    return {
      ownership: raw.ownership === "mine" ? "mine" : "all",
      workflow: ["all", ...WORKFLOW_FACETS].includes(raw.workflow) ? raw.workflow : "all",
    };
  }
  const legacy = {
    attention: "awaiting_me",
    open: "active",
    settled: "completed",
  };
  return { ownership: "all", workflow: legacy[raw] || "all" };
}

function number(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function optionalNumber(value) {
  if (value == null || value === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}
