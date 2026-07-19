import { button, compactId, el, empty, formatDate, present, proofState, projectedArray, status, statusLabel } from "./shared.js";

const expanded = new Set();
const COMPACT_LIMIT = 14;

export function timelineProjection(row) {
  const projected = projectedArray(row.timeline, "items", "events");
  const hasProjection = Array.isArray(row.timeline)
    || Array.isArray(row.timeline?.items)
    || Array.isArray(row.timeline?.events);
  if (hasProjection) {
    return {
      authoritative: true,
      items: projected.map((item, index) => normalizeProjected(item, index)),
    };
  }
  return { authoritative: false, items: fallbackTimeline(row) };
}

export function renderTimeline(row, projection, onProof, onRefresh) {
  const section = el("section", "inspectionBand timelineBand");
  const heading = el("header", "inspectionBandHeading");
  heading.append(
    el("div", "", "Timeline"),
    el("span", "inspectionProjectionState", projection.authoritative ? "Server timeline" : "Visible evidence fallback"),
  );
  section.append(heading);
  if (!projection.items.length) {
    section.append(empty("No visible Transfer history"));
    return section;
  }

  const isExpanded = expanded.has(row.uid);
  const items = isExpanded ? projection.items : projection.items.slice(-COMPACT_LIMIT);
  const list = el("ol", "transferTimeline");
  for (const item of items) list.append(timelineRow(item, onProof));
  section.append(list);
  if (projection.items.length > COMPACT_LIMIT) {
    const toggle = button(
      isExpanded ? "Show recent" : `Show all ${projection.items.length}`,
      "textButton timelineToggle",
      () => {
        if (isExpanded) expanded.delete(row.uid);
        else expanded.add(row.uid);
        onRefresh?.();
      },
    );
    toggle.setAttribute("aria-expanded", String(isExpanded));
    section.append(toggle);
  }
  return section;
}

function timelineRow(item, onProof) {
  const entry = el("li", "timelineEntry");
  entry.dataset.kind = item.kind;
  const marker = el("span", "timelineMarker");
  marker.setAttribute("aria-hidden", "true");
  const content = el("div", "timelineContent");
  const heading = el("div", "timelineEntryHeading");
  heading.append(el("strong", "", item.title), status(item.kind));
  const meta = [
    item.occurredAt ? formatDate(item.occurredAt) : "Time unavailable",
    item.person && `Person ${compactId(item.person)}`,
    item.revision != null && `Revision ${item.revision}`,
  ].filter(Boolean).join(" · ");
  content.append(heading, el("span", "timelineMeta", meta));
  if (item.fact || item.proofRaw) content.append(el("span", `timelineProofState ${item.proof.key}`, item.proof.label));
  if (item.summary) content.append(el("p", "timelineSummary", present(item.summary)));
  const refs = [
    item.targetUid && `Target ${compactId(item.targetUid)}`,
    item.fact && `Fact ${compactId(item.fact)}`,
    item.requestId && `Request ${compactId(item.requestId)}`,
  ].filter(Boolean);
  if (refs.length) content.append(el("span", "timelineRefs", refs.join(" · ")));
  const actions = el("div", "timelineEntryActions");
  if (item.fact || item.proofRaw) {
    const proof = button("Proof", "textButton timelineProofButton", () => onProof?.(item));
    proof.setAttribute("aria-label", `Inspect proof for ${item.title}`);
    actions.append(proof);
  }
  entry.append(marker, content, actions);
  return entry;
}

function normalizeProjected(raw, index) {
  const item = raw && typeof raw === "object" ? raw : {};
  const detail = item.detail && typeof item.detail === "object" ? item.detail : {};
  const proof = item.proof && typeof item.proof === "object" ? item.proof : {};
  const target = item.target && typeof item.target === "object" ? item.target : {};
  const fact = item.fact && typeof item.fact === "object" ? item.fact.uid : item.fact || item.fact_uid;
  const person = item.person || item.author_person || item.actor || item.actor_person || detail.person || proof.actor || null;
  const targetUid = target.uid || item.target_uid || item.record || item.transfer
    || detail.occurrence || detail.invitation || detail.party || detail.promise || detail.source
    || proof.record
    || (typeof item.target === "string" ? item.target : null);
  const kind = String(item.kind || item.type || item.event || "evidence");
  return {
    raw: item,
    uid: String(item.uid || item.event_uid || fact || `timeline-${index}`),
    kind,
    title: String(item.title || item.label || timelineTitle(kind, detail)),
    summary: item.summary ?? detailSummary(detail) ?? item.description ?? "",
    occurredAt: item.occurred_at || item.at || item.created_at || null,
    targetUid: referenceUid(targetUid),
    person: referenceUid(person),
    fact: fact || null,
    requestId: item.request_id || item.idempotency_key || detail.request_id || null,
    revision: item.revision ?? detail.revision ?? detail.winner_revision ?? detail.loser_revision ?? null,
    proofRaw: item.proof || item.proof_state || item.mechanism || null,
    proof: proofState(item.proof || item.proof_state || item.mechanism),
    links: projectedArray(item.links || item.linked_ids, "items"),
  };
}

function timelineTitle(kind, detail) {
  if (kind === "revision" && detail.revision != null) return `Revision ${detail.revision}`;
  if (kind === "invitation") return `Invitation ${statusLabel(detail.event || "changed")}`;
  if (kind === "agreement") return "Agreement level changed";
  if (kind === "activation") return "Promise activated";
  if (kind === "claim") {
    const role = statusLabel(detail.role || "role");
    return detail.asserted === false ? `${role} claim corrected` : `${role} claimed`;
  }
  if (kind === "settlement") return "Settlement slice recorded";
  if (kind === "dispute") return detail.asserted === false ? "Dispute retracted" : "Occurrence disputed";
  if (kind === "settlement_compensation") return "Settlement application compensated";
  if (kind === "first_completes_result") return "First-completes result";
  if (kind === "first_completes_loser") return "Sibling satiated";
  if (kind === "package_received") return "Network package received";
  if (kind === "package_seen") return "Network package seen";
  return statusLabel(kind);
}

function detailSummary(detail) {
  const entries = Object.entries(detail).filter(([key, value]) => value != null && ![
    "person", "request_id", "revision", "occurrence", "invitation", "party", "promise", "source",
  ].includes(key));
  if (!entries.length) return "";
  return entries.slice(0, 7).map(([key, value]) => `${statusLabel(key)}: ${present(value)}`).join(" · ");
}

function referenceUid(value) {
  if (!value || typeof value !== "object") return value || null;
  return value.uid || value.person_uid || value.record_uid || value.transfer_uid || null;
}

function fallbackTimeline(row) {
  const items = [];
  const add = (raw) => items.push(normalizeProjected(raw, items.length));
  const revision = row.revision_evidence || {};
  for (const evidence of [revision.previous, revision.current].filter(Boolean)) {
    add({
      uid: `revision:${evidence.revision}:${evidence.fact || "unknown"}`,
      kind: "revision",
      title: `Revision ${evidence.revision}`,
      summary: evidence.action ? statusLabel(evidence.action) : "Transfer terms recorded",
      at: evidence.at,
      actor: evidence.actor,
      fact: evidence.fact,
      revision: evidence.revision,
      proof_state: evidence.signature ? "direct_fact_signature" : evidence.proof_state,
      signature: evidence.signature,
      hash: evidence.hash,
      target_uid: row.uid,
    });
  }
  for (const invitation of row.invitations || []) {
    for (const event of invitationEvents(invitation)) {
      add({
        ...event,
        kind: `invitation_${event.kind || event.status || "changed"}`,
        title: `Invitation ${statusLabel(event.kind || event.status || "changed")}`,
        target_uid: invitation.uid,
        person: event.actor_person || event.actor,
        fact: event.fact,
        request_id: event.request_id,
      });
    }
    if (invitation.expires_at) add({
      uid: `invitation-expiry:${invitation.uid}:${invitation.expires_at}`,
      kind: "invitation_expiry",
      title: "Invitation expiry",
      summary: `${statusLabel(invitation.status)} · attempt ${invitation.attempt_number || 1}`,
      occurred_at: invitation.expires_at,
      target_uid: invitation.uid,
    });
  }
  for (const party of row.parties || []) {
    for (const event of party.agreement_events || []) add({
      ...event,
      kind: "agreement_level",
      title: "Agreement level changed",
      summary: `Level ${event.from_level ?? 0} to ${event.level ?? 0}`,
      person: event.person || party.actor,
      target_uid: party.uid || party.actor,
      fact: event.fact,
      request_id: event.request_id,
    });
  }
  for (const promise of row.promises || []) {
    for (const pair of promise.claim_pairs || []) add({
      ...pair,
      kind: "open_claim",
      title: "OPEN proposal claimed",
      summary: `${statusLabel(pair.reuse_policy || promise.reuse_policy)} source`,
      person: pair.claimant,
      target_uid: promise.uid,
      request_id: pair.request_id,
    });
  }
  for (const occurrence of row.occurrences || []) addOccurrence(add, row, occurrence);
  const group = row.first_completes;
  if (group) add({
    uid: group.evidence?.result?.uid || `first-completes:${row.uid}:${group.state}`,
    kind: "first_completes",
    title: "First-completes result",
    summary: [group.state, group.role, group.winner && `winner ${group.winner}`].filter(Boolean).map(statusLabel).join(" · "),
    at: group.evidence?.result?.at,
    fact: group.evidence?.result?.fact,
    target_uid: group.source || row.uid,
  });
  for (const confirmation of row.confirmations || []) add({
    ...confirmation,
    uid: confirmation.fact || `confirmation:${confirmation.kind}:${confirmation.at}`,
    kind: confirmation.kind || "confirmation",
    title: statusLabel(confirmation.kind || "confirmation"),
    fact: confirmation.fact,
    person: confirmation.actor,
    target_uid: row.uid,
  });
  return items.sort(compareTimeline);
}

function addOccurrence(add, row, occurrence) {
  if (occurrence.activation) add({
    uid: occurrence.activation.event || occurrence.activation.fact || `activation:${occurrence.uid}`,
    kind: "activation",
    title: "Promise activated",
    at: occurrence.activation.at,
    fact: occurrence.activation.fact,
    target_uid: occurrence.uid,
    revision: occurrence.revision,
  });
  for (const role of ["delivery", "receipt"]) {
    for (const event of occurrence[role]?.history || []) add({
      ...event,
      kind: event.claimed === false ? `${role}_correction` : `${role}_claim`,
      title: event.claimed === false ? `${statusLabel(role)} claim corrected` : `${statusLabel(role)} claimed`,
      target_uid: occurrence.uid,
      person: event.person,
      fact: event.fact,
      request_id: event.request_id,
      revision: occurrence.revision,
    });
  }
  for (const event of occurrence.dispute?.history || []) add({
    ...event,
    kind: event.disputed ? "participant_dispute" : "dispute_retracted",
    title: event.disputed ? "Occurrence disputed" : "Dispute retracted",
    target_uid: occurrence.uid,
    person: event.person,
    fact: event.fact,
    request_id: event.request_id,
  });
  if (occurrence.system_dispute?.disputed) add({
    uid: occurrence.system_dispute.fact || `system-dispute:${occurrence.uid}`,
    kind: "system_dispute",
    title: "System dispute detected",
    at: occurrence.system_dispute.at,
    fact: occurrence.system_dispute.fact,
    target_uid: occurrence.uid,
  });
  for (const slice of occurrence.settlement_progress?.slices || []) {
    add({
      uid: slice.uid,
      kind: "settlement",
      title: "Settlement slice recorded",
      summary: `Canonical ${slice.canonical_quantity ?? "?"} · remainder ${slice.remaining_after ?? "?"}`,
      at: slice.at,
      person: slice.owner,
      fact: slice.evidence_fact,
      request_id: slice.request_id,
      target_uid: occurrence.uid,
      linked_ids: [slice.local_record, slice.application_fact].filter(Boolean),
    });
    if (slice.compensation) add({
      ...slice.compensation,
      uid: slice.compensation.uid || `compensation:${slice.uid}`,
      kind: "settlement_correction",
      title: "Private settlement application compensated",
      fact: slice.compensation.fact,
      request_id: slice.compensation.request_id,
      target_uid: occurrence.uid,
    });
  }
}

function invitationEvents(invitation) {
  if (Array.isArray(invitation.events) && invitation.events.length) return invitation.events;
  return (invitation.attempts || []).flatMap((attempt) => Array.isArray(attempt.events) ? attempt.events : []);
}

function compareTimeline(left, right) {
  const leftAt = Date.parse(left.occurredAt || "") || Number.MAX_SAFE_INTEGER;
  const rightAt = Date.parse(right.occurredAt || "") || Number.MAX_SAFE_INTEGER;
  return leftAt - rightAt || left.uid.localeCompare(right.uid);
}
