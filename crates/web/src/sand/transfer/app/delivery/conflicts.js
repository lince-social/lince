import { blockers, capability, projectedAction, withActionInput } from "./model.js";
import { compactId, el, empty, formatDate, status, statusLabel } from "../inspection/shared.js";

const conflictReviews = new Map();

export function renderDeliveryConflicts(transfer, delivery, options) {
  const section = el("section", "deliveryEvidenceGroup deliveryConflictGroup");
  const heading = el("header", "deliveryGroupHeading");
  heading.append(el("strong", "", "Rejected local attempts"), status(delivery.conflicts.length ? "conflict" : "clear"));
  section.append(heading);
  if (!delivery.conflicts.length) {
    section.append(empty("No delivery conflicts"));
    return section;
  }
  const list = el("div", "deliveryConflictList");
  for (const conflict of delivery.conflicts) list.append(conflictRow(transfer, conflict, options));
  section.append(list);
  return section;
}

function conflictRow(transfer, conflict, options) {
  const id = conflict.uid || conflict.request_id || "unidentified";
  const block = el("article", "deliveryConflict");
  const heading = el("header", "deliveryConflictHeading");
  heading.append(
    el("strong", "", statusLabel(conflict.state)),
    el("span", "", conflict.at ? formatDate(conflict.at) : "Time unavailable"),
  );
  block.append(heading);

  const facts = el("dl", "deliveryConflictFacts");
  facts.append(
    fact("Attempt", conflict.request_id ? compactId(conflict.request_id) : "Unavailable"),
    fact("Submitted revision", conflict.submitted_revision ?? conflict.local_revision),
    fact("Authoritative revision", conflict.authoritative_revision ?? conflict.remote_revision),
    fact("Reason", conflict.message || conflict.reason || conflict.code),
  );
  block.append(facts);
  const retained = conflict.submitted_action ?? conflict.action_intent ?? conflict.retained_input;
  if (retained != null) {
    const details = el("details", "deliveryConflictInput");
    details.append(el("summary", "", "Retained submitted input"), el("pre", "", printable(retained)));
    block.append(details);
  }

  const actions = el("div", "deliveryConflictActions");
  appendSimpleAction(actions, transfer, conflict, "refresh", "Refresh authority", options);
  appendReviewedResubmit(actions, transfer, conflict, id, options);
  appendSimpleAction(actions, transfer, conflict, "discard_local_attempt", "Dismiss retained attempt", options);
  if (actions.childElementCount) block.append(actions);
  appendBlockers(block, blockers(conflict, "refresh", "resubmit_current", "discard_local_attempt"));
  return block;
}

function appendSimpleAction(root, transfer, conflict, name, label, options) {
  if (!capability(conflict, name)) return;
  const action = projectedAction(conflict, name);
  if (!action) return;
  const key = `delivery:${transfer.uid}:conflict:${conflict.uid || conflict.request_id}:${name}`;
  const button = actionButton(label, key, options);
  button.addEventListener("click", () => options.onAction?.(key, withActionInput(action)));
  root.append(button);
  appendActionError(root, key, options);
}

function appendReviewedResubmit(root, transfer, conflict, id, options) {
  if (!capability(conflict, "resubmit_current")) return;
  const action = projectedAction(conflict, "resubmit_current");
  if (!action) return;
  const reviewKey = `${transfer.uid}:${id}:resubmit`;
  const state = conflictReviews.get(reviewKey) || { reviewed: false };
  conflictReviews.set(reviewKey, state);
  const review = el("label", "deliveryReviewCheck");
  const check = el("input");
  check.type = "checkbox";
  check.checked = state.reviewed;
  check.disabled = options.mutationsEnabled === false;
  check.addEventListener("change", () => {
    state.reviewed = check.checked;
    sync();
  });
  review.append(check, el("span", "", "I reviewed the retained input against the authoritative revision and want to submit the server-provided current action."));
  const key = `delivery:${transfer.uid}:conflict:${id}:resubmit_current`;
  const button = actionButton("Submit reviewed action", key, options);
  const sync = () => {
    const actionState = options.actionState?.(key);
    button.disabled = options.mutationsEnabled === false
      || Boolean(actionState?.busy || actionState?.waiting)
      || !state.reviewed;
  };
  sync();
  button.addEventListener("click", () => options.onAction?.(key, withActionInput(action)));
  const group = el("div", "deliveryReviewedAction");
  group.append(review, button);
  appendActionError(group, key, options);
  root.append(group);
}

function actionButton(label, key, options) {
  const state = options.actionState?.(key);
  const button = el("button", "secondaryButton", state?.waiting ? "Awaiting live evidence" : state?.busy ? "Signing" : label);
  button.type = "button";
  button.disabled = options.mutationsEnabled === false || Boolean(state?.busy || state?.waiting);
  return button;
}

function appendActionError(root, key, options) {
  const message = options.actionState?.(key)?.error;
  if (!message) return;
  const alert = el("div", "inlineAlert", message);
  alert.setAttribute("role", "alert");
  root.append(alert);
}

function appendBlockers(root, values) {
  if (!values.length) return;
  const list = el("ul", "deliveryBlockers");
  for (const value of values) {
    list.append(el("li", "", statusLabel(value?.message || value?.code || value)));
  }
  root.append(list);
}

function fact(label, value) {
  const item = el("div", "deliveryFact");
  item.append(el("dt", "", label), el("dd", "", value == null || value === "" ? "Unavailable" : String(value)));
  return item;
}

function printable(value) {
  if (typeof value === "string") return value;
  try { return JSON.stringify(value, null, 2); } catch { return "Input unavailable"; }
}
