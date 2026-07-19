import {
  agreementLabel,
  balanceEntries,
  formatDate,
  formatQuantity,
  needsAttention,
  partyName,
  primaryStatus,
  progressLabel,
  promiseName,
  statusLabel,
} from "./model.js";
import {
  canClaimOpen,
  canCounteroffer,
  renderInvitationLifecycle,
  renderNegotiation,
} from "./negotiation.js";
import { renderAgreementControl, renderReadiness } from "./agreement.js";
import { renderAvailability, renderOccurrences, renderPromiseActivation } from "./occurrence.js";
import { renderBulkCompletion } from "./bulk.js";
import { renderHierarchyInspector } from "./hierarchy.js";
import { renderDetailEvidence } from "./inspection.js";
import { deliveryIndicator } from "./delivery/model.js";

export function renderSummary(root, summary) {
  root.replaceChildren(
    metric("Total", summary.total),
    metric("Awaiting me", summary.awaiting_me, summary.awaiting_me > 0 ? "danger" : ""),
    metric("Active", summary.active),
    metric("Completed", summary.completed, "success"),
  );
}

export function renderList(root, rows, selectedUid, onSelect, viewer = null) {
  const fragment = document.createDocumentFragment();
  for (const row of rows) fragment.append(transferCard(row, row.uid === selectedUid, onSelect, viewer));
  root.replaceChildren(fragment);
}

export function renderDetail(root, row, options) {
  const { onBack, onOpenRecord, onEdit, onCounteroffer, onClaim } = options;
  if (!row) {
    root.hidden = true;
    root.replaceChildren();
    return;
  }

  // Promises point at the Person record; agreement rows have their own uid
  // and expose that Person as `actor`.
  const partyByUid = new Map(row.parties.map((party) => [party.actor, party]));
  for (const invitation of row.invitations) {
    partyByUid.set(invitation.addressed_person, {
      actor: invitation.addressed_person,
      actor_head: invitation.addressed_person_head,
      actor_slug: invitation.addressed_person_slug,
      invitation_status: invitation.status,
    });
  }
  const header = el("header", "detailHeader");
  const back = button("Back", "backButton", onBack);
  back.setAttribute("aria-label", "Back to transfer overview");
  const heading = el("div", "detailHeading");
  heading.append(
    el("div", "eyebrow", row.slug || "Transfer"),
    el("h2", "", row.head),
    status(primaryStatus(row, options.viewer)),
  );
  header.append(back, heading);
  if (row.capabilities.edit_terms || row.capabilities.adopt_terms) {
    const edit = button(row.revision === 0 ? "Review and adopt" : "Edit", "secondaryButton detailEdit", onEdit);
    edit.disabled = options.mutationsEnabled === false;
    edit.title = row.revision === 0 ? "Review and seal these legacy terms as revision 1" : "Edit the complete signed draft";
    header.append(edit);
  }
  if (canCounteroffer(row)) {
    const counteroffer = button("Counteroffer", "secondaryButton detailEdit", () => onCounteroffer?.(row));
    counteroffer.disabled = options.mutationsEnabled === false;
    counteroffer.title = "Sign complete terms as the one current proposal";
    header.append(counteroffer);
  }

  const facts = el("dl", "factGrid");
  facts.append(
    fact("Agreement", agreementLabel(row)),
    fact("Revision", String(row.revision)),
    fact("Visibility", visibility(row)),
    fact("Settlement", statusLabel(row.settlement || "manual")),
    fact("Confirmation", row.require_confirmation ? "Delivery and receipt" : "Not required"),
    fact("Promise progress", progressLabel(row.progress)),
    fact("Occurrence settlement", occurrenceSettlementLabel(row.settlement_progress)),
    fact("Default reserve", statusLabel(row.reserve_default || "none")),
    fact("Default place", placeLabel(row.default_place)),
  );

  const readiness = detailSection("Agreement", "agreement-section");
  readiness.append(agreementMeter(row));
  const agreementPeople = el("div", "agreementPeople");
  for (const party of row.parties) {
    const item = el("div", "agreementPerson");
    item.append(
      el("span", "personName", partyName(party)),
      el("span", "levelLabel", agreementLevel(party.level)),
    );
    agreementPeople.append(item);
  }
  if (!row.parties.length) agreementPeople.append(emptyInline("No parties attached"));
  readiness.append(agreementPeople, renderAgreementControl(row, options));

  const revisions = revisionEvidenceSection(row);

  const invitations = renderInvitationLifecycle(row, options);

  const hierarchyInspector = renderHierarchyInspector(
    row,
    options.hierarchyRows || [row],
    options.onSelectBranch,
    options.viewer,
  );
  const inspection = renderDetailEvidence(row, options.hierarchyRows || [row], options);

  const promises = detailSection(`Promises (${row.promises.length})`, "promises-section");
  const promiseList = el("div", "promiseList");
  for (const promise of row.promises) {
    promiseList.append(promiseRow(row, promise, partyByUid.get(promise.party), onOpenRecord, onClaim, options));
  }
  if (!row.promises.length) promiseList.append(emptyInline("No promises attached"));
  promises.append(promiseList);

  const dependencies = dependencySection(row, onOpenRecord);
  const bulkCompletion = renderBulkCompletion(row, options.hierarchyRows || [row], options);
  const occurrences = renderOccurrences(row, options);

  const accounting = detailSection("Balance", "balance-section");
  const balanceList = el("div", "balanceList");
  const entries = balanceEntries(row);
  for (const [concept, value] of entries) {
    const item = el("div", "balanceRow");
    item.append(el("span", "", concept), el("strong", value === 0 ? "zero" : value > 0 ? "positive" : "negative", formatQuantity(value)));
    balanceList.append(item);
  }
  if (!entries.length) balanceList.append(emptyInline("No classified quantities"));
  accounting.append(balanceList);

  const confirmations = detailSection(`Evidence (${row.confirmations.length})`, "evidence-section");
  const evidenceList = el("div", "evidenceList");
  for (const confirmation of row.confirmations) {
    const item = el("div", "evidenceRow");
    item.append(
      el("strong", "", statusLabel(confirmation.kind)),
      el("span", "", confirmation.actor ? `Actor ${confirmation.actor}` : "Local actor"),
      el("time", "", formatDate(confirmation.at)),
    );
    evidenceList.append(item);
  }
  if (!row.confirmations.length) evidenceList.append(emptyInline("No confirmations recorded"));
  confirmations.append(evidenceList);

  const lineage = detailSection("Transfer tree", "lineage-section");
  const lineageGrid = el("div", "lineageGrid");
  lineageGrid.append(
    linkedReference("Parent", row.parent, row.parent_head || row.parent_slug, onOpenRecord),
    linkedReference("Source", row.source, row.source_head || row.source_slug, onOpenRecord),
    fact("Transfer ID", row.uid),
  );
  lineage.append(lineageGrid);

  const negotiation = renderNegotiation(row, options);

  root.hidden = false;
  root.replaceChildren(header, hierarchyInspector, facts, inspection, revisions, readiness, invitations, promises, dependencies, bulkCompletion, occurrences, accounting, confirmations, lineage);
  if (negotiation) root.append(negotiation);
}

function occurrenceSettlementLabel(progress) {
  if (!progress || Number(progress.occurrences) === 0) return "No occurrences";
  const statuses = Object.entries(progress.occurrence_statuses || {})
    .filter(([, count]) => Number(count) > 0)
    .map(([state, count]) => `${count} ${statusLabel(state)}`);
  return statuses.join(" · ") || `${progress.occurrences} occurrences`;
}

function transferCard(row, selected, onSelect, viewer) {
  const card = button("", "transferCard", () => onSelect(row.uid));
  const projectedStatus = primaryStatus(row, viewer);
  card.dataset.selected = String(selected);
  card.dataset.status = projectedStatus;
  card.setAttribute("aria-label", `Open ${row.head}`);

  const top = el("div", "cardTop");
  const identity = el("div", "cardIdentity");
  identity.append(el("strong", "cardTitle", row.head));
  if (row.slug) identity.append(el("span", "cardSlug", row.slug));
  const primary = status(projectedStatus);
  primary.classList.add("primaryStatus");
  top.append(identity, primary);
  const delivery = deliveryIndicator(row.social_delivery);
  if (delivery) {
    const indicator = el("span", "deliveryIndicator", delivery.label);
    indicator.dataset.state = delivery.state;
    top.append(indicator);
  }

  const agreement = agreementMeter(row, true);
  const acceptedPeople = row.parties.map(partyName).join(", ") || "No accepted parties";
  const invitedPeople = row.invitations.filter((invitation) => invitation.status === "pending").length;
  const promises = row.promises.length === 1 ? "1 promise" : `${row.promises.length} promises`;
  const occurrences = row.occurrences.length === 1 ? "1 occurrence" : `${row.occurrences.length} occurrences`;
  const meta = el("div", "cardMeta");
  meta.append(
    el("span", "", invitedPeople ? `${acceptedPeople} · ${invitedPeople} pending` : acceptedPeople),
    el("span", "", `${promises} · ${occurrences}`),
  );

  const promisePreview = el("div", "promisePreview");
  for (const promise of row.promises.slice(0, 3)) {
    const item = el("span", "promiseChip");
    item.append(
      el("b", Number(promise.delta) < 0 ? "out" : "in", formatQuantity(promise.delta, promise.unit_name)),
      document.createTextNode(` ${promiseName(promise)}`),
    );
    promisePreview.append(item);
  }
  if (row.promises.length > 3) promisePreview.append(el("span", "more", `+${row.promises.length - 3}`));

  if (needsAttention(row)) card.dataset.attention = "true";
  card.append(top, agreement, meta, promisePreview);
  return card;
}

function promiseRow(transfer, promise, party, onOpenRecord, onClaim, options) {
  const row = el("article", "promiseRow");
  const main = el("div", "promiseMain");
  const ownerLabel = party
    ? partyName(party)
    : promise.proposer_head || promise.proposer_slug || promise.proposer || promise.party || "Unavailable";
  main.append(
    el("strong", "promiseTitle", promiseName(promise)),
    el(
      "span",
      "promiseParty",
      promise.open
        ? `OPEN proposal by ${ownerLabel}`
        : party
        ? `${partyName(party)}${party.invitation_status ? ` · ${statusLabel(party.invitation_status)}` : ""}`
        : "Owner unavailable",
    ),
  );
  const amount = el("div", "promiseAmount");
  amount.append(
    el("strong", Number(promise.delta) < 0 ? "out" : "in", formatQuantity(promise.delta, promise.unit_name)),
    status(promise.state),
  );
  const schedule = el("div", "promiseSchedule");
  schedule.append(
    el("span", "", promise.window_start || promise.window_end
      ? `${promise.window_start ? formatDate(promise.window_start) : "Now"} to ${promise.window_end ? formatDate(promise.window_end) : "open-ended"}`
      : "No time window"),
    el("span", "", `Reserve from ${statusLabel(promise.reserve_from || "active")}`),
  );
  if (promise.open) schedule.append(el("span", "", `${statusLabel(promise.reuse_policy || "duplicate")} when claimed`));
  if (promise.source_promise) schedule.append(el("span", "", `From OPEN ${promise.source_promise}`));
  schedule.append(el("span", "", placeLabel(promise.place)));
  row.append(main, amount, schedule);
  if (promise.record) {
    const open = button("Open record", "textButton", () => onOpenRecord(promise.record));
    row.append(open);
  }
  if (canClaimOpen(transfer, promise)) {
    const claim = button(promise.reuse_policy === "consume" ? "Claim and consume" : "Claim a copy", "secondaryButton promiseClaim", () => onClaim?.(transfer, promise));
    claim.disabled = options.mutationsEnabled === false;
    claim.title = promise.reuse_policy === "consume" ? "Refine and assign this OPEN promise" : "Refine a new promise while keeping this OPEN source";
    row.append(claim);
  }
  const claimPairs = renderOpenClaimPairs(transfer, promise);
  if (claimPairs) row.append(claimPairs);
  const readiness = renderReadiness(promise.readiness, "Promise readiness");
  if (readiness) {
    readiness.classList.add("promiseReadiness");
    row.append(readiness);
  }
  const availability = renderAvailability(promise.availability, "Availability at activation");
  if (availability) row.append(availability);
  const activation = renderPromiseActivation(transfer, promise, options);
  if (activation) row.append(activation);
  return row;
}

function renderOpenClaimPairs(transfer, promise) {
  if (!Array.isArray(promise.claim_pairs) || !promise.claim_pairs.length) return null;
  const block = el("section", "openClaimPairs");
  block.append(el("strong", "openClaimPairsTitle", `Claimed pairs (${promise.claim_pairs.length})`));
  for (const pair of promise.claim_pairs) {
    const item = el("article", "openClaimPair");
    const heading = el("div", "openClaimPairHeading");
    heading.append(
      el("strong", "", `${pairPersonName(transfer, pair, "proposer")} and ${pairPersonName(transfer, pair, "claimant")}`),
      el("span", "", `Revision ${pair.revision}`),
    );
    const route = el("div", "openClaimPairRoute");
    route.append(
      el("span", "", `Proposer: ${pairPersonName(transfer, pair, "proposer")}`),
      el("span", "", "↔"),
      el("span", "", `Claimant: ${pairPersonName(transfer, pair, "claimant")}`),
    );
    const facts = el("dl", "openClaimPairFacts");
    facts.append(
      fact("Source OPEN promise", pair.source || promise.uid),
      fact("Proposer promise", pair.proposer_promise),
      fact("Claimant promise", pair.claimant_promise),
      fact("Source policy", statusLabel(pair.reuse_policy || promise.reuse_policy)),
      fact("Request ID", pair.request_id),
      fact("Claimed at", pair.at ? formatDate(pair.at) : "Unavailable"),
    );
    item.append(
      heading,
      route,
      el("span", "openClaimPairAgreement", "Concrete pair; proposer and claimant agreement gates later stages."),
      facts,
    );
    block.append(item);
  }
  return block;
}

function pairPersonName(transfer, pair, role) {
  const uid = pair?.[role];
  const party = transfer.parties.find((candidate) => candidate.actor === uid || candidate.uid === uid);
  return party ? partyName(party) : pair?.[`${role}_head`] || pair?.[`${role}_slug`] || uid || "Unavailable";
}

function agreementMeter(row, compact = false) {
  const block = el("div", compact ? "agreementMeter compact" : "agreementMeter");
  const label = el("div", "meterLabel");
  label.append(el("span", "", agreementLabel(row)), el("span", "", row.agreement.policy_satisfied ? "Agreed" : "Waiting"));
  const track = el("div", "meterTrack");
  const fill = el("span", "meterFill");
  const denominator = row.agreement.required || row.agreement.total || 1;
  fill.style.width = `${Math.min(100, (row.agreement.committed / denominator) * 100)}%`;
  track.append(fill);
  block.append(label, track);
  return block;
}

function status(value) {
  const pill = el("span", "status", statusLabel(value));
  pill.dataset.status = value || "draft";
  return pill;
}

function metric(label, value, tone = "") {
  const item = el("div", "metric");
  if (tone) item.dataset.tone = tone;
  item.append(el("span", "", label), el("strong", "", String(value)));
  return item;
}

function detailSection(title, id) {
  const section = el("section", "detailSection");
  section.id = id;
  section.append(el("h3", "", title));
  return section;
}

function fact(label, value) {
  const item = el("div", "fact");
  item.append(el("dt", "", label), el("dd", "", value || "None"));
  return item;
}

function linkedReference(label, uid, name, onOpenRecord) {
  if (!uid) return fact(label, "None");
  const item = el("div", "fact");
  const reference = button(name || uid, "referenceButton", () => onOpenRecord(uid));
  reference.title = uid;
  item.append(el("dt", "", label), reference);
  return item;
}

function visibility(row) {
  if (row.visibility === "proximity" && row.max_proximity != null) {
    return `Proximity ${row.max_proximity}`;
  }
  return statusLabel(row.visibility);
}

function agreementLevel(level) {
  if (Number(level) >= 2) return "Agreed";
  if (Number(level) >= 1) return "Checked · ready to agree";
  return "No agreement";
}

function dependencySection(row, onOpenRecord) {
  const dependencies = detailSection(`Dependencies (${row.dependencies.length})`, "dependencies-section");
  const list = el("div", "dependencyList");
  for (const dependency of row.dependencies) {
    const item = el("article", "dependencyRow");
    const scopedPromise = row.promises.find((promise) => promise.uid === dependency.promise);
    const scope = dependency.scope === "promise"
      ? `Promise · ${scopedPromise ? promiseName(scopedPromise) : dependency.promise}`
      : "Whole transfer";
    const actual = dependency.actual_state || (Array.isArray(dependency.actual_states) ? dependency.actual_states.map(statusLabel).join(", ") : "");
    item.append(
      el("strong", "", scope),
      el("span", "", `${statusLabel(dependency.upstream_kind)} must be ${statusLabel(dependency.required_state)}${actual ? ` · now ${statusLabel(actual)}` : ""}`),
      status(dependency.satisfied ? "ready" : "blocked"),
    );
    if (dependency.upstream_kind === "transfer") {
      const reference = button(dependency.upstream_head || dependency.upstream_slug || dependency.upstream, "referenceButton", () => onOpenRecord(dependency.upstream));
      item.append(reference);
    } else {
      item.append(el("span", "dependencyUpstream", dependency.upstream_head || dependency.upstream));
    }
    list.append(item);
  }
  if (!row.dependencies.length) list.append(emptyInline("No dependency gates"));
  dependencies.append(list);
  return dependencies;
}

function revisionEvidenceSection(row) {
  const section = detailSection("Revision evidence", "revision-evidence-section");
  const projection = row.revision_evidence && typeof row.revision_evidence === "object" ? row.revision_evidence : {};
  const current = projection.current;
  if (!current) {
    section.append(emptyInline(row.revision === 0 ? "Legacy terms have not been reviewed and sealed." : "Signed revision evidence is unavailable."));
    return section;
  }
  const evidence = el("div", "evidenceList");
  const currentRow = el("div", "evidenceRow");
  currentRow.append(
    el("strong", "", `Revision ${current.revision}`),
    el("span", "", current.signed ? "Signed revision" : "Unsigned revision"),
    el("time", "", formatDate(current.at)),
  );
  evidence.append(currentRow);
  if (projection.previous) {
    const previous = el("div", "evidenceRow");
    previous.append(
      el("strong", "", `Previous ${projection.previous.revision}`),
      el("span", "", projection.previous.signed ? "Signed revision" : "Unsigned revision"),
      el("time", "", formatDate(projection.previous.at)),
    );
    evidence.append(previous);
  }
  const changed = Array.isArray(projection.changed_fields) ? projection.changed_fields : [];
  if (changed.length) evidence.append(el("div", "revisionChanges", `Changed: ${changed.map(humanField).join(", ")}`));
  section.append(evidence);
  return section;
}

function humanField(value) {
  return statusLabel(String(value || "").replaceAll(".", " "));
}

function placeLabel(place) {
  if (!place || typeof place !== "object") return "No place";
  const address = String(place.address || "").trim();
  const coordinates = place.lat != null && place.lon != null && String(place.lat) !== "" && String(place.lon) !== ""
    && Number.isFinite(Number(place.lat)) && Number.isFinite(Number(place.lon))
    ? `${Number(place.lat)}, ${Number(place.lon)}` : "";
  return [address, coordinates].filter(Boolean).join(" · ") || "No place";
}

function emptyInline(text) {
  return el("div", "emptyInline", text);
}

function button(text, className, onClick) {
  const node = el("button", className, text);
  node.type = "button";
  node.addEventListener("click", onClick);
  return node;
}

function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}
