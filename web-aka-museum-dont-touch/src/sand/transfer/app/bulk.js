import { branchRows, rootUidFor } from "./hierarchy.js";
import { selectedActingPerson, setSelectedActingPerson } from "./occurrence.js";
import { formatDate, partyName, statusLabel } from "./model.js";

const selectedOccurrences = new Map();
const lastSubmissions = new Map();
let activePreview = emptyPreviewState();

export function bulkCompletionScope(row, rows, options) {
  if (!row) return null;
  const person = selectedActingPerson(row, options);
  const branch = branchRows(rows, row.uid);
  const candidates = completionCandidates(branch, person);
  const selectionKey = `${row.uid}:${person}`;
  let selected = selectedOccurrences.get(selectionKey);
  if (!selected) {
    selected = new Set(candidates.filter((candidate) => !candidate.currentClaimed).map((candidate) => candidate.occurrence.uid));
    selectedOccurrences.set(selectionKey, selected);
  }
  const candidateUids = new Set(candidates.map((candidate) => candidate.occurrence.uid));
  for (const uid of [...selected]) if (!candidateUids.has(uid)) selected.delete(uid);
  return {
    branch,
    branchUid: row.uid,
    candidates,
    person,
    rootUid: rootUidFor(rows, row.uid),
    selected,
    selectionKey,
  };
}

export function syncBulkCompletionPreview(scope, onChange) {
  const occurrenceUids = scope ? [...scope.selected].sort() : [];
  const key = scope?.person && occurrenceUids.length
    ? `${scope.rootUid}:${scope.person}:${occurrenceUids.join(",")}` : "";
  if (activePreview.key === key) return;
  activePreview.unsubscribe?.();
  activePreview = emptyPreviewState(key);
  if (!key) return;
  const host = window.LinceWidgetHost;
  if (typeof host?.subscribeProtein !== "function") {
    activePreview.loading = false;
    activePreview.error = "Live bulk review is unavailable";
    return;
  }
  activePreview.loading = true;
  const expectedKey = key;
  activePreview.unsubscribe = host.subscribeProtein(
    "transfer-bulk-completion-preview",
    {
      source: "transfer_bulk_completion_preview",
      where: [
        { uid_eq: scope.rootUid },
        { occurrence_in: occurrenceUids },
      ],
    },
    (payload) => {
      if (activePreview.key !== expectedKey) return;
      const projected = Array.isArray(payload?.rows) ? payload.rows[0] : null;
      const preview = normalizeBulkPreview(projected);
      if (!preview || preview.root !== scope.rootUid || preview.person !== scope.person) {
        activePreview.preview = null;
        activePreview.error = "No exact bulk review is available for this Person and selection";
      } else {
        activePreview.preview = preview;
        activePreview.error = "";
      }
      activePreview.loading = false;
      onChange?.();
    },
  );
}

export function renderBulkCompletion(row, rows, options) {
  const scope = bulkCompletionScope(row, rows, options);
  const section = detailSection("Reviewed bulk completion", "bulk-completion-section");
  const controls = el("div", "bulkScopeControls");
  const personOptions = branchPeople(scope.branch);
  if (options.viewer?.local) {
    const picker = el("select", "bulkPersonSelect");
    picker.append(new Option("Select acting Person", ""));
    for (const party of personOptions) picker.append(new Option(partyName(party), party.actor));
    picker.value = scope.person;
    picker.addEventListener("change", () => {
      setSelectedActingPerson(row, picker.value);
      options.onBulkSelectionChange?.();
    });
    controls.append(labeled("Acting Person", picker));
  } else {
    controls.append(el("span", "bulkActingPerson", `Acting Person: ${personName(personOptions, scope.person)}`));
  }
  controls.append(el("span", "bulkBranchScope", `${scope.branch.length} transfer${scope.branch.length === 1 ? "" : "s"} in selected branch`));
  section.append(controls);

  if (!scope.person) {
    section.append(emptyInline("Select one acting Person to review their missing delivery or receipt claims."));
    return section;
  }
  if (!scope.candidates.length) {
    section.append(emptyInline("This Person has no occurrence role in the selected branch."));
    return section;
  }

  const selection = el("div", "bulkCandidateList");
  for (const candidate of scope.candidates) {
    const item = el("label", "bulkCandidate");
    const checkbox = el("input", "");
    checkbox.type = "checkbox";
    checkbox.checked = scope.selected.has(candidate.occurrence.uid);
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) scope.selected.add(candidate.occurrence.uid);
      else scope.selected.delete(candidate.occurrence.uid);
      options.onBulkSelectionChange?.();
    });
    const identity = el("span", "bulkCandidateIdentity");
    identity.append(
      el("strong", "", candidate.transfer.head),
      el("span", "", `${candidate.roleLabel} · ${candidate.occurrence.uid}`),
    );
    item.append(checkbox, identity, status(candidate.currentClaimed ? "claimed" : "missing"));
    selection.append(item);
  }
  section.append(selection);

  if (!scope.selected.size) {
    section.append(emptyInline("Select at least one occurrence for focused server review."));
    appendLastSubmission(section, scope, null, options);
    return section;
  }
  if (activePreview.loading) {
    section.append(el("div", "bulkPreviewState", "Reviewing exact claim state"));
    appendLastSubmission(section, scope, null, options);
    return section;
  }
  if (activePreview.error || !activePreview.preview) {
    section.append(el("div", "bulkPreviewError", activePreview.error || "Waiting for the focused server review"));
    appendLastSubmission(section, scope, null, options);
    return section;
  }

  const preview = activePreview.preview;
  const review = el("section", "bulkReview");
  const heading = el("div", "bulkReviewHeading");
  heading.append(
    el("strong", "", "Server-reviewed completion plan"),
    status(preview.eligible ? "ready" : "blocked"),
  );
  review.append(heading);
  const summary = el("dl", "bulkReviewSummary");
  summary.append(
    fact("Selected", String(preview.item_count)),
    fact("Eligible", String(preview.eligible_count)),
    fact("Blocked", String(preview.blocked_count)),
    fact("Root revision", String(preview.root_revision)),
    fact("Acting Person", personName(personOptions, preview.person)),
    fact("Review token", preview.review_token),
  );
  review.append(summary);

  const items = el("ol", "bulkReviewItems");
  for (const item of preview.items) items.append(reviewItem(item, rows));
  review.append(items);
  if (preview.blockers.length) review.append(el("div", "bulkPreviewBlockers", preview.blockers.map(blockerLabel).join(" · ")));

  const eligibleItems = preview.items.filter((item) => item.eligible);
  const payloadItems = eligibleItems.map((item) => item.action);
  const reviewedSelection = [...preview.selection].sort();
  const requestedSelection = [...scope.selected].sort();
  const exactReview = preview.eligible
    && reviewedSelection.length === requestedSelection.length
    && reviewedSelection.every((uid, index) => uid === requestedSelection[index])
    && eligibleItems.every(actionMatchesReview)
    && payloadItems.length === preview.item_count
    && payloadItems.length === preview.eligible_count
    && Boolean(preview.review_token);
  if (exactReview) {
    const key = `bulk:${scope.rootUid}:${scope.branchUid}:${scope.person}:${preview.review_token}`;
    const actionState = options.actionState?.(key) || null;
    const actionControls = el("div", "bulkActionControls");
    const acknowledgement = el("label", "bulkAcknowledgement");
    const checkbox = el("input", "");
    checkbox.type = "checkbox";
    checkbox.disabled = options.mutationsEnabled === false
      || Boolean(actionState?.busy || actionState?.waiting);
    acknowledgement.append(
      checkbox,
      el("span", "", `Confirm ${payloadItems.length} individually attributable claim${payloadItems.length === 1 ? "" : "s"} for ${personName(personOptions, scope.person)}`),
    );
    const submit = actionButton("Complete reviewed claims", key, options);
    submit.disabled = true;
    checkbox.addEventListener("change", () => {
      submit.disabled = options.mutationsEnabled === false
        || !checkbox.checked || Boolean(actionState?.busy || actionState?.waiting);
    });
    submit.addEventListener("click", () => {
      if (!checkbox.checked || submit.disabled) return;
      lastSubmissions.set(scope.selectionKey, {
        at: new Date().toISOString(),
        items: preview.items.map((item) => ({ ...item })),
        key,
        reviewToken: preview.review_token,
      });
      options.onBulkAction?.(key, {
        action: "complete-transfer-occurrence-claims-bulk",
        request_id: requestId(),
        ...(options.viewer?.local ? { person: scope.person } : {}),
        review_token: preview.review_token,
        items: payloadItems,
      }, scope.branch);
    });
    actionControls.append(acknowledgement, submit);
    review.append(actionControls);
    appendActionError(review, key, options);
  }
  section.append(review);
  appendLastSubmission(section, scope, preview, options);
  return section;
}

function completionCandidates(rows, person) {
  if (!person) return [];
  const candidates = [];
  for (const transfer of rows) {
    for (const occurrence of transfer.occurrences) {
      const delivery = occurrence.giver === person;
      const receipt = occurrence.receiver === person;
      if (!delivery && !receipt) continue;
      const ownedClaims = [
        delivery ? Boolean(occurrence.delivery?.claimed) : null,
        receipt ? Boolean(occurrence.receipt?.claimed) : null,
      ].filter((value) => value != null);
      candidates.push({
        occurrence,
        transfer,
        currentClaimed: ownedClaims.every(Boolean),
        roleLabel: [
          delivery ? `Delivery ${occurrence.delivery?.claimed ? "claimed" : "missing"}` : null,
          receipt ? `Receipt ${occurrence.receipt?.claimed ? "claimed" : "missing"}` : null,
        ].filter(Boolean).join(" · "),
      });
    }
  }
  return candidates.sort((left, right) => left.transfer.head.localeCompare(right.transfer.head)
    || left.occurrence.uid.localeCompare(right.occurrence.uid));
}

function normalizeBulkPreview(raw) {
  if (!raw || typeof raw !== "object" || raw.kind !== "transfer_bulk_completion_preview") return null;
  return {
    ...raw,
    root: String(raw.root || ""),
    root_revision: Number(raw.root_revision || 0),
    person: String(raw.person || ""),
    review_token: String(raw.review_token || ""),
    selection: Array.isArray(raw.selection) ? raw.selection.map(String) : [],
    eligible: Boolean(raw.eligible),
    item_count: Number(raw.item_count || 0),
    eligible_count: Number(raw.eligible_count || 0),
    blocked_count: Number(raw.blocked_count || 0),
    blockers: Array.isArray(raw.blockers) ? raw.blockers : [],
    items: (Array.isArray(raw.items) ? raw.items : []).map((item) => ({
      ...item,
      occurrence: String(item?.occurrence || ""),
      transfer: item?.transfer == null ? null : String(item.transfer),
      transfer_revision: item?.transfer_revision == null ? null : Number(item.transfer_revision),
      role: item?.role == null ? null : String(item.role),
      current_claimed: Boolean(item?.current_claimed),
      eligible: Boolean(item?.eligible),
      blockers: Array.isArray(item?.blockers) ? item.blockers : [],
      action: item?.action && typeof item.action === "object" ? item.action : null,
    })),
  };
}

function reviewItem(item, rows) {
  const row = el("li", "bulkReviewItem");
  const transfer = rows.find((candidate) => candidate.uid === item.transfer);
  const heading = el("div", "bulkReviewItemHeading");
  heading.append(
    el("strong", "", transfer?.head || item.transfer || "Unknown transfer"),
    status(item.eligible ? "ready" : item.current_claimed ? "claimed" : "blocked"),
  );
  row.append(
    heading,
    el("span", "bulkReviewItemMeta", [statusLabel(item.role || "no_role"), item.occurrence, `revision ${item.transfer_revision ?? "?"}`].join(" · ")),
  );
  if (item.claim_token) row.append(el("span", "bulkReviewItemToken", `Claim token ${item.claim_token}`));
  if (item.blockers.length) row.append(el("span", "bulkReviewItemBlockers", item.blockers.map(blockerLabel).join(" · ")));
  return row;
}

function appendLastSubmission(section, scope, preview, options) {
  const submission = lastSubmissions.get(scope.selectionKey);
  if (!submission) return;
  const block = el("section", "bulkResultHistory");
  block.append(el("strong", "", "Last bulk submission"), el("time", "", formatDate(submission.at)));
  const list = el("ol", "bulkResultItems");
  const actionState = options.actionState?.(submission.key) || null;
  for (const submitted of submission.items) {
    const live = preview?.items.find((item) => item.occurrence === submitted.occurrence && item.role === submitted.role);
    const projected = projectedClaim(scope.branch, submitted);
    const observed = Boolean(live?.current_claimed || projected.claimed);
    const state = observed ? "Observed complete"
      : actionState?.error ? "Not applied"
        : actionState?.busy || actionState?.waiting ? "Awaiting live evidence" : "Awaiting inspection";
    const item = el("li", "bulkResultItem");
    item.append(
      el("strong", "", `${statusLabel(submitted.role)} · ${submitted.occurrence}`),
      el("span", "", [state, (live?.claim_token || projected.token) && `Claim token ${live?.claim_token || projected.token}`, actionState?.error].filter(Boolean).join(" · ")),
    );
    list.append(item);
  }
  block.append(list);
  section.append(block);
}

function projectedClaim(rows, submitted) {
  for (const transfer of rows) {
    const occurrence = transfer.occurrences.find((candidate) => candidate.uid === submitted.occurrence);
    if (!occurrence) continue;
    const claim = submitted.role === "delivery" ? occurrence.delivery
      : submitted.role === "receipt" ? occurrence.receipt : null;
    const latest = claim?.history?.[claim.history.length - 1];
    return {
      claimed: Boolean(claim?.claimed),
      token: latest?.uid || claim?.fact || "",
    };
  }
  return { claimed: false, token: "" };
}

function validBulkActionItem(item) {
  return item && typeof item === "object"
    && typeof item.occurrence === "string" && item.occurrence.length > 0
    && typeof item.transfer === "string" && item.transfer.length > 0
    && Number.isInteger(item.expected_revision) && item.expected_revision >= 0
    && ["delivery", "receipt"].includes(item.role)
    && typeof item.expected_delivery_claimed === "boolean"
    && typeof item.expected_receipt_claimed === "boolean";
}

function actionMatchesReview(item) {
  const action = item.action;
  return validBulkActionItem(action)
    && action.occurrence === item.occurrence
    && action.transfer === item.transfer
    && action.expected_revision === item.transfer_revision
    && action.role === item.role;
}

function branchPeople(rows) {
  const people = new Map();
  for (const row of rows) {
    for (const party of row.parties) if (party.actor) people.set(party.actor, party);
  }
  return [...people.values()].sort((left, right) => partyName(left).localeCompare(partyName(right)));
}

function personName(people, uid) {
  const party = people.find((candidate) => candidate.actor === uid || candidate.uid === uid);
  return party ? partyName(party) : uid || "Unavailable";
}

function actionButton(text, key, options) {
  const state = options.actionState?.(key) || null;
  const button = el("button", "primaryButton");
  button.type = "button";
  button.textContent = state?.waiting ? "Waiting for live evidence" : state?.busy ? "Signing" : text;
  button.disabled = options.mutationsEnabled === false || Boolean(state?.waiting || state?.busy);
  return button;
}

function appendActionError(block, key, options) {
  const message = options.actionState?.(key)?.error;
  if (!message) return;
  const error = el("div", "inlineAlert", message);
  error.setAttribute("role", "alert");
  block.append(error);
}

function requestId() {
  if (globalThis.crypto?.randomUUID) return `transfer-bulk-complete:${globalThis.crypto.randomUUID()}`;
  return `transfer-bulk-complete:${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function blockerLabel(value) {
  if (value && typeof value === "object") return statusLabel(value.message || value.code || value.kind || "blocked");
  return statusLabel(value || "blocked");
}

function emptyPreviewState(key = "") {
  return { key, loading: false, error: "", preview: null, unsubscribe: null };
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

function labeled(text, control) {
  const label = el("label", "inlineField bulkPersonField");
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

function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}
