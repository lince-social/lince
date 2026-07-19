import { formatDate, formatQuantity, normalizeSettlementPreview, partyName, promiseName, statusLabel } from "./model.js";

const localSelection = new Map();
const settlementPreviewEditors = new Map();
const SETTLEMENT_PREVIEW_DEBOUNCE_MS = 350;

export function syncSettlementPreviewSubscriptions(activeOccurrenceUids) {
  const active = new Set(activeOccurrenceUids || []);
  for (const [occurrenceUid, editor] of settlementPreviewEditors) {
    if (active.has(occurrenceUid)) continue;
    clearTimeout(editor.timer);
    editor.unsubscribe?.();
    settlementPreviewEditors.delete(occurrenceUid);
  }
}

export function renderOccurrences(row, options) {
  const section = detailSection(`Occurrences (${row.occurrences.length})`, "occurrences-section");
  let person = selectedActingPerson(row, options);
  const controls = el("div", "occurrenceControls");

  if (options.viewer?.local && row.parties.length && hasOccurrenceActions(row)) {
    const picker = el("select", "occurrencePersonSelect");
    picker.append(new Option("Select acting Person", ""));
    for (const party of row.parties) picker.append(new Option(partyName(party), party.actor));
    picker.value = person;
    picker.addEventListener("change", () => {
      localSelection.set(row.uid, picker.value);
      section.replaceWith(renderOccurrences(row, options));
    });
    controls.append(labeled("Acting Person", picker));
    person = picker.value;
  }
  if (controls.childElementCount) section.append(controls);

  const paths = groupByExchangePath(row.occurrences);
  const list = el("div", "occurrencePathList");
  for (const [path, occurrences] of paths) {
    const pathBlock = el("section", "occurrencePath");
    const pathHeading = el("header", "occurrencePathHeader");
    pathHeading.append(
      el("strong", "", occurrences.length > 1 ? "Exchange" : "Directed occurrence"),
      el("span", "", path || "One-sided path"),
    );
    pathBlock.append(pathHeading);
    for (const occurrence of occurrences) pathBlock.append(occurrenceRow(row, occurrence, person, options));
    list.append(pathBlock);
  }
  if (!row.occurrences.length) list.append(emptyInline("No occurrences activated"));
  section.append(list);
  return section;
}

export function renderPromiseActivation(transfer, promise, options) {
  const canActivate = promise.capabilities?.activate === true;
  const blockers = blockerValues(promise.blocking_reasons?.activate);
  if (!canActivate && !blockers.length) return null;

  const block = el("div", "promiseActivation");
  let person = selectedActingPerson(transfer, options);
  if (options.viewer?.local) {
    const picker = el("select", "occurrencePersonSelect");
    picker.append(new Option("Select acting Person", ""));
    for (const party of transfer.parties) picker.append(new Option(partyName(party), party.actor));
    picker.value = person;
    picker.addEventListener("change", () => {
      localSelection.set(transfer.uid, picker.value);
      block.replaceWith(renderPromiseActivation(transfer, promise, options));
    });
    block.append(labeled("Acting Person", picker));
    person = picker.value;
  }

  if (canActivate) {
    const key = `occurrence:${transfer.uid}:activate:${promise.uid}:${person}`;
    const activate = actionButton("Activate occurrence", key, options, true);
    activate.disabled ||= Boolean(options.viewer?.local && !person);
    activate.addEventListener("click", () => options.onAction?.(key, {
      action: "activate-transfer-occurrence",
      transfer: transfer.uid,
      promise: promise.uid,
      expected_revision: transfer.revision,
      request_id: requestId("activate"),
      person: options.viewer?.local ? person : null,
    }));
    block.append(activate);
    appendActionError(block, key, options);
  }
  if (blockers.length) block.append(el("div", "activationBlockers", blockers.map(blockerLabel).join(" · ")));
  return block;
}

export function selectedActingPerson(row, options) {
  if (!options.viewer?.local) return options.viewer?.person || row.viewer_party?.actor || "";
  return localSelection.get(row.uid) || "";
}

export function setSelectedActingPerson(row, person) {
  localSelection.set(row.uid, String(person || ""));
}

export function renderAvailability(raw, label = "Availability") {
  if (!raw || typeof raw !== "object") return null;
  const fields = ["actual", "available", "reserved", "planned", "surplus"]
    .filter((name) => raw[name] != null);
  if (!fields.length) return null;
  const block = el("div", "availabilityBlock");
  block.append(el("strong", "availabilityTitle", label));
  const grid = el("dl", "availabilityGrid");
  for (const name of fields) {
    const item = el("div", "availabilityMetric");
    item.append(el("dt", "", statusLabel(name)), el("dd", "", formatQuantity(raw[name], raw.unit_name || raw.unit)));
    grid.append(item);
  }
  block.append(grid);
  const unknown = Array.isArray(raw.unknown_units) ? raw.unknown_units : Array.isArray(raw.availability_unknown_units) ? raw.availability_unknown_units : [];
  if (unknown.length) block.append(el("div", "availabilityUnknown", `Unknown units: ${unknown.map(unknownUnitLabel).join(", ")}`));
  return block;
}

function occurrenceRow(transfer, occurrence, person, options) {
  const row = el("article", "occurrenceRow");
  const heading = el("div", "occurrenceHeading");
  const identity = el("div", "occurrenceIdentity");
  identity.append(
    el("strong", "", occurrence.subject_head || occurrence.subject_slug || occurrence.subject || occurrence.uid),
    el("span", "", `Revision ${occurrence.revision} · ${occurrence.uid}`),
  );
  heading.append(identity, status(occurrenceStatus(occurrence)));
  row.append(heading);

  const route = el("div", "occurrenceRoute");
  route.append(
    personReference(transfer, occurrence.giver, "Giver"),
    el("span", "occurrenceArrow", "→"),
    personReference(transfer, occurrence.receiver, "Receiver"),
  );
  row.append(route);

  const facts = el("dl", "occurrenceFacts");
  const promise = transfer.promises.find((candidate) => candidate.uid === occurrence.promise);
  facts.append(
    fact("Quantity", formatQuantity(occurrence.settlement_progress?.canonical_quantity ?? occurrence.quantity)),
    fact("Promise", occurrence.promise_head || (promise ? promiseName(promise) : occurrence.promise) || "None"),
    fact("Record UID", occurrence.subject),
    fact("Concept UID", occurrence.concept),
    fact("Unit UID", occurrence.unit),
    fact("Window", occurrenceWindow(occurrence)),
    fact("Place", placeLabel(occurrence.place || occurrence.location)),
  );
  row.append(facts);

  if (occurrence.activation) row.append(activationEvidence(occurrence.activation));

  const availability = renderAvailability(occurrence.availability, "Engine availability");
  if (availability) row.append(availability);
  if (occurrence.reservation && typeof occurrence.reservation === "object") {
    const reservation = el("dl", "reservationFacts");
    reservation.append(
      fact("Reservation", statusLabel(occurrence.reservation.policy || occurrence.reservation.reserve_from || "none")),
      fact("Policy source", statusLabel(occurrence.reservation.source || "code default")),
    );
    row.append(reservation);
  }

  const claims = el("div", "occurrenceClaims");
  claims.append(claimBlock(transfer, occurrence, "delivery", occurrence.delivery, person, options));
  claims.append(claimBlock(transfer, occurrence, "receipt", occurrence.receipt, person, options));
  row.append(claims);

  if (occurrence.has_application_formula || occurrence.capabilities?.set_application_formula === true) {
    row.append(formulaBlock(transfer, occurrence, person, options));
  }
  if (occurrence.settlement_progress) {
    row.append(settlementProgressBlock(transfer, occurrence, person, options));
  }
  if (occurrence.settlement_preview) {
    row.append(settlementReviewBlock(transfer, occurrence, person, options));
  }
  row.append(participantDisputeBlock(transfer, occurrence, person, options));
  if (hasSystemDisputeEvidence(occurrence.system_dispute)) row.append(systemDisputeBlock(occurrence.system_dispute));
  return row;
}

function settlementProgressBlock(transfer, occurrence, person, options) {
  const progress = occurrence.settlement_progress;
  const block = el("section", "settlementProgress");
  const heading = el("div", "settlementProgressHeading");
  heading.append(
    el("strong", "", "Settlement progress"),
    status(progress.settled ? "settled" : progress.partially_settled ? "partially_settled" : "active"),
  );
  block.append(heading);

  const facts = el("dl", "settlementProgressFacts");
  facts.append(
    fact("Canonical total", quantityLabel(progress.canonical_quantity, occurrence)),
    fact("Settled", quantityLabel(progress.settled_quantity, occurrence)),
    fact("Remaining", quantityLabel(progress.remaining_quantity, occurrence)),
  );
  block.append(facts);

  if (progress.canonical_quantity != null && progress.canonical_quantity > 0) {
    const meter = el("div", "settlementMeter");
    const fill = el("span", "settlementMeterFill");
    const fraction = Math.max(0, Math.min(1, (progress.settled_quantity || 0) / progress.canonical_quantity));
    fill.style.width = `${fraction * 100}%`;
    meter.setAttribute("role", "progressbar");
    meter.setAttribute("aria-valuemin", "0");
    meter.setAttribute("aria-valuemax", String(progress.canonical_quantity));
    meter.setAttribute("aria-valuenow", String(progress.settled_quantity || 0));
    meter.append(fill);
    block.append(meter);
  }

  if (progress.slices.length) {
    const history = el("ol", "settlementHistory");
    for (const [index, slice] of progress.slices.entries()) {
      history.append(settlementSlice(transfer, occurrence, slice, index, person, options));
    }
    block.append(history);
  }
  return block;
}

function settlementSlice(transfer, occurrence, slice, index, person, options) {
  const item = el("li", "settlementSlice");
  const heading = el("div", "settlementSliceHeading");
  const owner = transfer.parties.find((party) => party.actor === slice.owner || party.uid === slice.owner);
  heading.append(
    el("strong", "", `Slice ${index + 1}`),
    el("span", "", slice.at ? formatDate(slice.at) : "Immutable evidence"),
  );
  item.append(heading);

  const publicFacts = el("dl", "settlementSliceFacts");
  publicFacts.append(
    fact("Canonical quantity", quantityLabel(slice.canonical_quantity, occurrence)),
    fact("Owner", owner ? partyName(owner) : slice.owner),
    fact("Cumulative after", quantityLabel(slice.cumulative_after, occurrence)),
    fact("Remaining after", quantityLabel(slice.remaining_after, occurrence)),
    fact("Evidence Fact", slice.evidence_fact),
    fact("Remainder", slice.remainder_policy ? statusLabel(slice.remainder_policy) : "Unavailable"),
    fact("Settlement UID", slice.uid),
    fact("Request ID", slice.request_id),
  );
  item.append(publicFacts);

  if (slice.application_formula_hash) {
    item.append(evidenceLine("Formula fingerprint", `${slice.application_formula_hash} · version ${slice.application_formula_version ?? 0}`));
  }
  if (slice.local_record || slice.local_delta != null || slice.application_fact) {
    const privateFacts = el("dl", "settlementSlicePrivate");
    privateFacts.append(
      fact("Private local Record", slice.local_record),
      fact("Private local delta", slice.local_delta == null ? "Unavailable" : formatQuantity(slice.local_delta)),
      fact("Local cumulative before", slice.local_cumulative_before == null ? "Unavailable" : formatQuantity(slice.local_cumulative_before)),
      fact("Local cumulative after", slice.local_cumulative_after == null ? "Unavailable" : formatQuantity(slice.local_cumulative_after)),
      fact("Private formula", slice.application_formula),
      fact("Application Fact", slice.application_fact),
    );
    item.append(privateFacts);
  }
  if (slice.compensation_status || slice.compensation || slice.capabilities?.compensate === true) {
    item.append(settlementCompensationBlock(slice, person, options));
  }
  return item;
}

function settlementCompensationBlock(slice, person, options) {
  const block = el("section", "settlementCompensation");
  const heading = el("div", "settlementCompensationHeading");
  heading.append(
    el("strong", "", "Private application correction"),
    status(slice.compensation_status || (slice.compensation ? "compensated" : "applied")),
  );
  block.append(heading);

  if (slice.compensation) {
    const evidence = el("dl", "settlementCompensationFacts");
    evidence.append(
      fact("Compensation UID", slice.compensation.uid),
      fact("Compensation Fact", slice.compensation.fact),
      fact("Original application Fact", slice.compensation.original_application_fact),
      fact("Private local Record", slice.compensation.local_record),
      fact("Inverse private delta", slice.compensation.inverse_delta == null ? "Unavailable" : formatQuantity(slice.compensation.inverse_delta)),
      fact("Request ID", slice.compensation.request_id),
      fact("Compensated at", slice.compensation.at ? formatDate(slice.compensation.at) : "Unavailable"),
    );
    block.append(evidence);
  }

  if (slice.capabilities?.compensate === true) {
    const key = `settlement:${slice.uid}:compensate:${person}`;
    const state = options.actionState?.(key) || null;
    const controls = el("div", "settlementCompensationControls");
    const acknowledgement = el("label", "settlementAcknowledgement");
    const checkbox = el("input", "");
    checkbox.type = "checkbox";
    const localOwnerSelected = !options.viewer?.local || Boolean(person && person === slice.owner);
    checkbox.disabled = options.mutationsEnabled === false
      || Boolean(state?.busy || state?.waiting || !localOwnerSelected);
    acknowledgement.append(checkbox, el("span", "", "Confirm reversal of this private application"));
    const compensate = actionButton("Compensate private application", key, options, true);
    compensate.disabled = true;
    checkbox.addEventListener("change", () => {
      compensate.disabled = options.mutationsEnabled === false
        || !checkbox.checked || Boolean(state?.busy || state?.waiting || !localOwnerSelected);
    });
    compensate.addEventListener("click", () => {
      if (!checkbox.checked || compensate.disabled) return;
      options.onAction?.(key, {
        action: "compensate-transfer-occurrence-settlement",
        settlement: slice.uid,
        request_id: requestId("compensate"),
        ...(options.viewer?.local ? { person } : {}),
      });
    });
    controls.append(acknowledgement, compensate);
    block.append(controls);
    appendActionError(block, key, options);
  }

  const blockers = blockerValues(slice.blocking_reasons?.compensate);
  if (blockers.length) block.append(el("div", "settlementBlockers", blockers.map(blockerLabel).join(" · ")));
  return block;
}

function settlementReviewBlock(transfer, occurrence, person, options) {
  const staticPreview = occurrence.settlement_preview;
  const editor = settlementPreviewEditor(occurrence, staticPreview);
  const preview = editor.dirty ? editor.preview : staticPreview;
  const sourcePromise = transfer.promises.find((promise) => promise.uid === occurrence.promise);
  const sourceOwner = staticPreview.owner || sourcePromise?.party || null;
  const block = el("section", "settlementReview");
  const heading = el("div", "settlementReviewHeading");
  heading.append(
    el("strong", "", "Irreversible settlement review"),
    el("span", "", "Append-only slice"),
  );
  block.append(heading);

  const quantityField = el("label", "settlementQuantityField");
  const quantityInput = el("input", "");
  quantityInput.type = "number";
  quantityInput.min = "0";
  quantityInput.step = "any";
  quantityInput.inputMode = "decimal";
  quantityInput.value = editor.quantity;
  if (occurrence.settlement_progress?.remaining_quantity != null) {
    quantityInput.max = String(occurrence.settlement_progress.remaining_quantity);
  }
  const previewState = el("span", "settlementPreviewState", settlementPreviewStateLabel(editor));
  quantityField.append(el("span", "", "Canonical settlement slice"), quantityInput, previewState);
  block.append(quantityField);

  if (!preview) {
    block.append(el("div", "settlementBlockers", editor.error || "Waiting for an exact server preview"));
    bindSettlementQuantityInput(quantityInput, previewState, occurrence, staticPreview, editor, options);
    return block;
  }

  const facts = el("dl", "settlementReviewFacts");
  facts.append(
    fact("Canonical slice", quantityLabel(preview.canonical_quantity, occurrence)),
    fact("Remaining before", quantityLabel(preview.remaining_quantity, occurrence)),
    fact("Already settled", quantityLabel(preview.settled_quantity, occurrence)),
  );
  if (preview.remaining_after != null) {
    facts.append(fact("Remaining after", quantityLabel(preview.remaining_after, occurrence)));
  }
  facts.append(
    fact("Source owner", sourceOwner ? partyName(transfer.parties.find((party) => party.actor === sourceOwner) || { actor: sourceOwner }) : "Unavailable"),
    fact("Private local Record", preview.local_record),
    fact("Private local delta", preview.local_delta == null ? "Unavailable" : formatQuantity(preview.local_delta)),
    fact("Private cumulative after", preview.local_cumulative_after == null ? "Unavailable" : formatQuantity(preview.local_cumulative_after)),
    fact("Formula version", preview.application_formula_version == null ? "Unavailable" : String(preview.application_formula_version)),
    fact("Remainder policy", preview.remainder_policy ? statusLabel(preview.remainder_policy) : "Unavailable"),
  );
  block.append(facts);
  if (preview.application_formula_hash) {
    block.append(evidenceLine("Formula fingerprint", preview.application_formula_hash));
  }

  const canSettle = editor.dirty
    ? preview.capabilities?.settle === true
    : occurrence.capabilities?.settle === true;
  const previewReady = settlementPreviewReady(preview);
  const localOwnerSelected = !options.viewer?.local
    || Boolean(person && (!sourceOwner || person === sourceOwner));
  if (canSettle) {
    const key = `occurrence:${occurrence.uid}:settle:${person}:${preview.remaining_quantity}:${preview.canonical_quantity}`;
    const state = options.actionState?.(key) || null;
    const controls = el("div", "settlementReviewControls");
    const acknowledgement = el("label", "settlementAcknowledgement");
    const checkbox = el("input", "");
    checkbox.type = "checkbox";
    checkbox.disabled = options.mutationsEnabled === false
      || Boolean(state?.busy || state?.waiting || !previewReady || !localOwnerSelected);
    acknowledgement.append(checkbox, el("span", "", "Confirm this exact settlement slice"));
    const settle = actionButton("Settle reviewed slice", key, options, true);
    settle.disabled = true;
    checkbox.addEventListener("change", () => {
      settle.disabled = options.mutationsEnabled === false
        || !checkbox.checked || Boolean(state?.busy || state?.waiting || !previewReady || !localOwnerSelected);
    });
    settle.addEventListener("click", () => {
      if (!checkbox.checked || settle.disabled) return;
      options.onAction?.(key, {
        action: "settle-transfer-occurrence",
        occurrence: occurrence.uid,
        request_id: requestId("settle"),
        person: options.viewer?.local ? person : null,
        canonical_quantity: preview.canonical_quantity,
        expected_remaining_quantity: preview.expected_remaining_quantity,
        expected_local_delta: preview.expected_local_delta,
        expected_application_formula_hash: preview.expected_application_formula_hash,
        expected_application_formula_version: preview.expected_application_formula_version,
        expected_remainder_policy: preview.expected_remainder_policy,
      });
    });
    controls.append(acknowledgement, settle);
    block.append(controls);
    appendActionError(block, key, options);
  }

  const blockers = [...blockerValues(occurrence.blocking_reasons?.settle)];
  if (!previewReady) blockers.push("settlement preview is incomplete");
  if (blockers.length) block.append(el("div", "settlementBlockers", blockers.map(blockerLabel).join(" · ")));
  bindSettlementQuantityInput(quantityInput, previewState, occurrence, staticPreview, editor, options);
  return block;
}

function settlementPreviewEditor(occurrence, staticPreview) {
  let editor = settlementPreviewEditors.get(occurrence.uid);
  if (!editor) {
    editor = {
      dirty: false,
      error: "",
      loading: false,
      preview: null,
      quantity: String(staticPreview.canonical_quantity),
      requestedQuantity: null,
      timer: null,
      unsubscribe: null,
    };
    settlementPreviewEditors.set(occurrence.uid, editor);
  } else if (!editor.dirty) {
    editor.quantity = String(staticPreview.canonical_quantity);
  }
  return editor;
}

function bindSettlementQuantityInput(input, stateLabel, occurrence, staticPreview, editor, options) {
  input.addEventListener("input", () => {
    editor.quantity = input.value;
    editor.error = "";
    const quantity = Number(input.value);
    clearTimeout(editor.timer);
    editor.unsubscribe?.();
    editor.unsubscribe = null;

    if (Number.isFinite(quantity) && quantity === staticPreview.canonical_quantity) {
      editor.dirty = false;
      editor.loading = false;
      editor.preview = null;
      editor.requestedQuantity = null;
      stateLabel.textContent = settlementPreviewStateLabel(editor);
      options.onSettlementPreviewChange?.();
      return;
    }

    editor.dirty = true;
    editor.preview = null;
    editor.loading = Number.isFinite(quantity) && quantity > 0;
    editor.requestedQuantity = quantity;
    stateLabel.textContent = settlementPreviewStateLabel(editor);
    disableSettlementControls(input.closest(".settlementReview"));
    if (!editor.loading) {
      editor.error = "Enter a positive canonical quantity";
      stateLabel.textContent = editor.error;
      return;
    }

    editor.timer = setTimeout(() => {
      requestSettlementPreview(occurrence.uid, quantity, editor, options);
    }, SETTLEMENT_PREVIEW_DEBOUNCE_MS);
  });
}

function requestSettlementPreview(occurrenceUid, quantity, editor, options) {
  const host = window.LinceWidgetHost;
  if (typeof host?.subscribeProtein !== "function") {
    editor.loading = false;
    editor.error = "Live settlement preview is unavailable";
    options.onSettlementPreviewChange?.();
    return;
  }
  const subscriptionId = `transfer-settlement-preview:${occurrenceUid}`;
  editor.unsubscribe = host.subscribeProtein(
    subscriptionId,
    {
      source: "transfer_settlement_preview",
      where: [{ uid_eq: occurrenceUid }, { quantity_eq: quantity }],
    },
    ({ rows }) => {
      if (editor.requestedQuantity !== quantity) return;
      const projected = Array.isArray(rows) ? rows[0] : null;
      const preview = normalizeSettlementPreview(projected?.settlement_preview || projected);
      const projectedOccurrence = projected?.occurrence || projected?.occurrence_uid;
      if (!preview
        || (projectedOccurrence && projectedOccurrence !== occurrenceUid)
        || preview.canonical_quantity !== quantity) {
        editor.preview = null;
        editor.error = "No exact settlement preview is available";
      } else {
        editor.preview = preview;
        editor.error = "";
      }
      editor.loading = false;
      options.onSettlementPreviewChange?.();
    },
  );
}

function settlementPreviewStateLabel(editor) {
  if (editor.error) return editor.error;
  if (editor.loading) return "Waiting for server preview";
  if (editor.dirty && editor.preview) return "Exact server preview";
  return "Full remaining preview";
}

function disableSettlementControls(block) {
  if (!block) return;
  block.dataset.previewStale = "true";
  for (const control of block.querySelectorAll(".settlementAcknowledgement input, .settlementReviewControls button")) {
    control.disabled = true;
  }
}

function settlementPreviewReady(preview) {
  return Number.isFinite(preview.canonical_quantity)
    && preview.canonical_quantity > 0
    && Number.isFinite(preview.remaining_quantity)
    && preview.remaining_quantity >= preview.canonical_quantity
    && Number.isFinite(preview.local_delta)
    && preview.expected_remaining_quantity === preview.remaining_quantity
    && preview.expected_local_delta === preview.local_delta
    && typeof preview.expected_application_formula_hash === "string"
    && preview.expected_application_formula_hash.length > 0
    && preview.expected_application_formula_hash === preview.application_formula_hash
    && Number.isInteger(preview.expected_application_formula_version)
    && preview.expected_application_formula_version >= 0
    && preview.expected_application_formula_version === preview.application_formula_version
    && typeof preview.expected_remainder_policy === "string"
    && preview.expected_remainder_policy.length > 0
    && preview.expected_remainder_policy === preview.remainder_policy
    && Boolean(preview.local_record);
}

function claimBlock(transfer, occurrence, role, claim, person, options) {
  const block = el("section", "occurrenceClaim");
  const claimed = Boolean(claim?.claimed);
  const title = role === "delivery" ? "Delivery" : "Receipt";
  block.append(
    el("strong", "", title),
    el("span", "claimState", claimed ? "Claimed" : "Not claimed"),
  );
  if (claim?.actor || claim?.person || claim?.fact || claim?.at) {
    block.append(el("span", "claimEvidence", [
      claim.person_head || claim.actor_head || claim.person || claim.actor,
      claim.fact && `Fact ${claim.fact}`,
      claim.at ? formatDate(claim.at) : "",
    ].filter(Boolean).join(" · ")));
  }
  const rolePerson = role === "delivery" ? occurrence.giver : occurrence.receiver;
  const capability = claimed ? `correct_${role}` : `confirm_${role}`;
  if ((!options.viewer?.local || person === rolePerson) && occurrence.capabilities?.[capability] === true) {
    const key = `occurrence:${occurrence.uid}:${role}:${claimed ? "retract" : "claim"}:${person}`;
    const control = actionButton(claimed ? `Retract ${title.toLowerCase()}` : `Confirm ${title.toLowerCase()}`, key, options);
    control.addEventListener("click", () => options.onAction?.(key, {
      action: "set-transfer-occurrence-claim",
      occurrence: occurrence.uid,
      request_id: requestId(role),
      person: options.viewer?.local ? person : null,
      role,
      claimed: !claimed,
    }));
    block.append(control);
    appendActionError(block, key, options);
  }
  const history = Array.isArray(claim?.history) ? claim.history : [];
  if (history.length) {
    const evidence = el("ol", "claimHistory");
    for (const event of history) {
      const item = el("li", "");
      item.append(
        el("strong", "", event.claimed ? "Claimed" : "Retracted"),
        el("span", "", [
          event.person || event.actor,
          event.fact && `Fact ${event.fact}`,
          event.at ? formatDate(event.at) : "",
        ].filter(Boolean).join(" · ")),
      );
      evidence.append(item);
    }
    block.append(evidence);
  }
  return block;
}

function formulaBlock(transfer, occurrence, person, options) {
  const block = el("section", "applicationFormula");
  block.append(el("strong", "", "Private application formula"));
  if (occurrence.has_application_formula) block.append(el("code", "formulaValue", occurrence.application_formula || "incoming()"));
  if (occurrence.application) {
    const result = el("dl", "applicationFacts");
    result.append(
      fact("Formula source", statusLabel(occurrence.application.source)),
      fact("Applied private delta", occurrence.application.applied_local_delta == null ? "Unavailable" : formatQuantity(occurrence.application.applied_local_delta)),
    );
    block.append(result);
    if (occurrence.application.error) {
      const error = el("div", "inlineAlert", occurrence.application.error);
      error.setAttribute("role", "alert");
      block.append(error);
    }
  }
  if (occurrence.capabilities?.set_application_formula === true
    && (!options.viewer?.local || person === occurrence.receiver)) {
    const form = el("form", "formulaForm");
    const input = el("input", "");
    input.type = "text";
    input.value = occurrence.application_formula || "incoming()";
    input.autocomplete = "off";
    const key = `occurrence:${occurrence.uid}:formula:${person}`;
    const submit = actionButton("Save formula", key, options);
    submit.type = "submit";
    form.append(input, submit);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const formula = input.value.trim();
      if (!formula || submit.disabled) return;
      options.onAction?.(key, {
        action: "set-transfer-occurrence-application-formula",
        occurrence: occurrence.uid,
        request_id: requestId("formula"),
        person: options.viewer?.local ? person : null,
        formula,
      });
    });
    block.append(form);
    appendActionError(block, key, options);
  }
  return block;
}

function activationEvidence(activation) {
  const block = el("section", "occurrenceEvidence");
  block.append(
    el("strong", "", "Activation evidence"),
    evidenceLine("Event", activation.event),
    evidenceLine("Fact", activation.fact),
  );
  if (activation.at) block.append(el("time", "", formatDate(activation.at)));
  return block;
}

function participantDisputeBlock(transfer, occurrence, person, options) {
  const dispute = occurrence.dispute;
  const block = el("section", "participantDispute");
  const heading = el("div", "participantDisputeHeading");
  heading.append(
    el("strong", "", "Participant dispute"),
    status(dispute.participant_disputed ? "participant_disputed" : "clear"),
  );
  block.append(heading);

  if (dispute.history.length) {
    const history = el("ol", "participantDisputeHistory");
    for (const event of dispute.history) {
      const actor = transfer.parties.find((party) => party.actor === event.person || party.uid === event.person);
      const item = el("li", "participantDisputeEvent");
      item.append(
        el("strong", "", event.disputed ? "Dispute raised" : "Dispute retracted"),
        el("span", "", [
          event.person ? (actor ? partyName(actor) : event.person) : "Actor unavailable",
          event.fact && `Fact ${event.fact}`,
          event.request_id && `Request ${event.request_id}`,
          event.at ? formatDate(event.at) : "",
        ].filter(Boolean).join(" · ")),
      );
      history.append(item);
    }
    block.append(history);
  } else {
    block.append(el("span", "participantDisputeEmpty", "No participant dispute history"));
  }

  const canRetract = occurrence.capabilities?.retract_dispute === true;
  const canDispute = occurrence.capabilities?.dispute === true;
  if (canRetract || canDispute) {
    const disputed = canRetract;
    const key = `occurrence:${occurrence.uid}:dispute:${disputed ? "retract" : "raise"}:${person}`;
    const control = actionButton(disputed ? "Retract my dispute" : "Raise dispute", key, options);
    const participantSelected = !options.viewer?.local
      || Boolean(person && (person === occurrence.giver || person === occurrence.receiver));
    control.disabled ||= !participantSelected;
    control.addEventListener("click", () => options.onAction?.(key, {
      action: "set-transfer-occurrence-dispute",
      occurrence: occurrence.uid,
      request_id: requestId("dispute"),
      ...(options.viewer?.local ? { person } : {}),
      disputed: !disputed,
    }));
    block.append(control);
    appendActionError(block, key, options);
  }

  const blockerKey = canRetract ? "retract_dispute" : "dispute";
  const blockers = blockerValues(occurrence.blocking_reasons?.[blockerKey]);
  if (!canDispute && !canRetract && blockers.length) {
    block.append(el("div", "participantDisputeBlockers", blockers.map(blockerLabel).join(" · ")));
  }
  return block;
}

function hasSystemDisputeEvidence(dispute) {
  return Boolean(dispute?.disputed || dispute?.fact || dispute?.at);
}

function systemDisputeBlock(dispute) {
  const block = el("section", "systemDispute");
  const heading = el("div", "systemDisputeHeading");
  heading.append(el("strong", "", "System safety hold"), status("system_disputed"));
  block.append(heading, el("span", "systemDisputeDescription", "Settlement is held by a system integrity check."));
  if (dispute.fact) block.append(evidenceLine("Safety hold Fact", dispute.fact));
  if (dispute.at) block.append(el("time", "", formatDate(dispute.at)));
  return block;
}

function evidenceLine(label, value) {
  const line = el("span", "evidenceLine");
  line.append(el("b", "", label), document.createTextNode(` ${value || "Unavailable"}`));
  return line;
}

function hasOccurrenceActions(row) {
  return row.occurrences.some((occurrence) => ["confirm_delivery", "correct_delivery", "confirm_receipt", "correct_receipt", "set_application_formula", "settle", "settle_occurrence", "dispute", "retract_dispute"]
    .some((name) => occurrence.capabilities?.[name] === true)
    || occurrence.settlement_progress?.slices.some((slice) => slice.capabilities?.compensate === true));
}

function occurrenceStatus(occurrence) {
  if (occurrence.system_disputed) return "system_disputed";
  if (occurrence.disputed) return "participant_disputed";
  if (occurrence.settlement_progress?.settled) return "settled";
  if (occurrence.settlement_progress?.partially_settled) return "partially_settled";
  if (occurrence.confirmed_conclusion) return "confirmed_conclusion";
  return occurrence.state;
}

function quantityLabel(value, occurrence) {
  if (value == null) return "Unavailable";
  return formatQuantity(value, occurrence.unit_name || occurrence.unit);
}

function groupByExchangePath(occurrences) {
  const paths = new Map();
  for (const occurrence of occurrences) {
    const key = occurrence.path || `one-sided:${occurrence.uid}`;
    if (!paths.has(key)) paths.set(key, []);
    paths.get(key).push(occurrence);
  }
  return paths;
}

function actionButton(text, key, options, primary = false) {
  const state = options.actionState?.(key) || null;
  const control = el("button", primary ? "primaryButton" : "secondaryButton");
  control.type = "button";
  control.textContent = state?.waiting ? "Waiting for live occurrence" : state?.busy ? "Signing" : text;
  control.disabled = options.mutationsEnabled === false || Boolean(state?.waiting || state?.busy);
  if (state?.error) control.title = state.error;
  return control;
}

function appendActionError(block, key, options) {
  const message = options.actionState?.(key)?.error;
  if (!message) return;
  const error = el("div", "inlineAlert", message);
  error.setAttribute("role", "alert");
  block.append(error);
}

function personReference(transfer, uid, role) {
  const party = transfer.parties.find((candidate) => candidate.actor === uid || candidate.uid === uid);
  const block = el("div", "occurrencePerson");
  block.append(el("span", "", role), el("strong", "", party ? partyName(party) : uid || "Unassigned"));
  return block;
}

function occurrenceWindow(occurrence) {
  if (!occurrence.window_start && !occurrence.window_end) return "No time window";
  return `${occurrence.window_start ? formatDate(occurrence.window_start) : "Now"} to ${occurrence.window_end ? formatDate(occurrence.window_end) : "open-ended"}`;
}

function placeLabel(place) {
  if (!place || typeof place !== "object") return "No place";
  const coordinates = place.lat != null && place.lon != null ? `${place.lat}, ${place.lon}` : "";
  return [place.address, coordinates].filter(Boolean).join(" · ") || "No place";
}

function fact(label, value) {
  const item = el("div", "fact");
  item.append(el("dt", "", label), el("dd", "", value || "None"));
  return item;
}

function status(value) {
  const pill = el("span", "status", value === "confirmed_conclusion" ? "Confirmed conclusion" : statusLabel(value || "active"));
  pill.dataset.status = value || "active";
  return pill;
}

function labeled(text, control) {
  const label = el("label", "inlineField occurrencePersonField");
  label.append(el("span", "", text), control);
  return label;
}

function detailSection(title, id) {
  const section = el("section", "detailSection");
  section.id = id;
  section.append(el("h3", "", title));
  return section;
}

function emptyInline(text) { return el("div", "emptyInline", text); }

function blockerValues(raw) {
  if (Array.isArray(raw)) return raw;
  return raw ? [raw] : [];
}

function blockerLabel(blocker) {
  if (blocker && typeof blocker === "object") return statusLabel(blocker.message || blocker.code || blocker.kind || "blocked");
  return statusLabel(blocker || "blocked");
}

function unknownUnitLabel(value) {
  if (!value || typeof value !== "object") return String(value);
  const unit = value.unit || value.unit_name || "unknown unit";
  const recordUnit = value.record_unit || value.record_unit_name;
  const promise = value.promise || value.promise_head;
  return [unit, recordUnit ? `record uses ${recordUnit}` : "", promise ? `promise ${promise}` : ""].filter(Boolean).join(" · ");
}

function requestId(kind) {
  if (globalThis.crypto?.randomUUID) return `transfer-${kind}:${globalThis.crypto.randomUUID()}`;
  return `transfer-${kind}:${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}
