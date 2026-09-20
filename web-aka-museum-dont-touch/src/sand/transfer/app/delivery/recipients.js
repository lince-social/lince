import {
  blockers,
  capability,
  projectedAction,
  recipientKey,
  withActionInput,
} from "./model.js";
import { compactId, el, empty, formatDate, status, statusLabel } from "../inspection/shared.js";

const drafts = new Map();
const recipientForms = new Map();

export function renderDeliveryRecipients(transfer, delivery, options) {
  const section = el("section", "deliveryRecipientGroup");
  const heading = el("header", "deliveryGroupHeading");
  heading.append(
    el("strong", "", "Recipients"),
    el("span", "", `${delivery.recipients.length} explicit`),
  );
  section.append(heading);
  const configure = configureRecipient(transfer, delivery, options);
  if (configure) section.append(configure);
  if (!delivery.recipients.length) {
    section.append(empty("No social-delivery recipients in this projection"));
    return section;
  }
  const list = el("div", "deliveryRecipientList");
  for (const recipient of delivery.recipients) {
    list.append(recipientRow(transfer, delivery, recipient, options));
  }
  section.append(list);
  return section;
}

function configureRecipient(transfer, delivery, options) {
  const operation = capability(delivery, "add_recipient") ? "add_recipient"
    : capability(delivery, "configure_recipient") ? "configure_recipient"
      : capability(delivery, "configure") ? "configure" : null;
  if (!operation || !delivery.eligible_recipients.length) return null;
  const template = projectedAction(delivery, operation);
  if (!template) return null;
  const formState = recipientForms.get(transfer.uid) || { recipient: "", mode: "", reviewed: false };
  recipientForms.set(transfer.uid, formState);

  const form = el("form", "deliveryRecipientForm");
  const personField = el("label", "deliveryRecipientField");
  personField.append(el("span", "", "Person and Organ"));
  const recipientSelect = el("select");
  recipientSelect.append(new Option("Choose an eligible recipient", ""));
  for (const recipient of delivery.eligible_recipients) {
    recipientSelect.append(new Option(recipientOptionLabel(recipient), recipientKey(recipient)));
  }
  recipientSelect.value = formState.recipient;
  personField.append(recipientSelect);

  const modeField = el("label", "deliveryRecipientField");
  modeField.append(el("span", "", "Persistent mode"));
  const modeSelect = el("select");
  modeField.append(modeSelect);
  const review = el("label", "deliveryReviewCheck");
  const reviewCheck = el("input");
  reviewCheck.type = "checkbox";
  review.append(reviewCheck, el("span", "", "I reviewed the exact disclosure and want this recipient to retain a redacted replica."));
  const key = `delivery:${transfer.uid}:configure-recipient`;
  const submit = actionButton("Sign recipient delivery", key, options, "primaryButton");
  submit.type = "submit";

  const selected = () => delivery.eligible_recipients
    .find((recipient) => recipientKey(recipient) === formState.recipient) || null;
  const renderModes = () => {
    const recipient = selected();
    const modes = recipient?.available_modes || [];
    modeSelect.replaceChildren(new Option("Choose server-approved mode", ""));
    for (const mode of modes) modeSelect.append(new Option(statusLabel(mode), mode));
    if (!modes.includes(formState.mode)) {
      formState.mode = modes.includes(recipient?.default_mode) ? recipient.default_mode : "";
    }
    modeSelect.value = formState.mode;
    review.hidden = formState.mode !== "replicated";
    reviewCheck.checked = formState.reviewed;
    sync();
  };
  const sync = () => {
    const state = options.actionState?.(key);
    submit.disabled = options.mutationsEnabled === false
      || Boolean(state?.busy || state?.waiting)
      || !selected() || !formState.mode
      || (formState.mode === "replicated" && !formState.reviewed);
  };
  recipientSelect.addEventListener("change", () => {
    formState.recipient = recipientSelect.value;
    formState.mode = "";
    formState.reviewed = false;
    renderModes();
  });
  modeSelect.addEventListener("change", () => {
    formState.mode = modeSelect.value;
    formState.reviewed = false;
    reviewCheck.checked = false;
    review.hidden = formState.mode !== "replicated";
    sync();
  });
  reviewCheck.addEventListener("change", () => {
    formState.reviewed = reviewCheck.checked;
    sync();
  });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (submit.disabled) return;
    const recipient = selected();
    const action = withActionInput(template, {
      recipient_person: recipient.person,
      recipient_organ: recipient.organ,
      mode: formState.mode,
    });
    if (action) options.onAction?.(key, action);
  });
  form.append(personField, modeField, review, submit);
  renderModes();
  appendActionError(form, key, options);
  return form;
}

function recipientRow(transfer, delivery, recipient, options) {
  const key = recipientKey(recipient);
  const block = el("article", "deliveryRecipient");
  block.dataset.deliveryState = recipient.state || "unknown";
  const heading = el("header", "deliveryRecipientHeading");
  const identity = el("div", "deliveryRecipientIdentity");
  identity.append(
    el("strong", "", recipient.person_head || recipient.person_slug || recipient.person || "Person unavailable"),
    el("span", "", recipient.organ_head || recipient.organ_slug || recipient.organ
      ? `Organ ${recipient.organ_head || recipient.organ_slug || compactId(recipient.organ)}` : "Organ unavailable"),
  );
  const states = el("div", "deliveryRecipientStates");
  if (recipient.contact_state) states.append(status(recipient.contact_state));
  states.append(status(recipient.state || recipient.policy_state || "unknown"));
  heading.append(identity, states);
  block.append(heading, recipientFacts(recipient));

  const mode = modeControl(transfer, delivery, recipient, key, options);
  if (mode) block.append(mode);
  const actions = deliveryActions(transfer, delivery, recipient, key, options);
  if (actions.childElementCount) block.append(actions);
  appendBlockers(block, combinedBlockers(delivery, recipient));
  return block;
}

function recipientFacts(recipient) {
  const facts = el("dl", "deliveryRecipientFacts");
  facts.append(
    fact("Mode", recipient.mode ? statusLabel(recipient.mode) : "Unavailable"),
    fact("Attempts", recipient.attempts),
    fact("Last attempt", recipient.last_attempt_at ? formatDate(recipient.last_attempt_at) : null),
    fact("Acknowledged", recipient.acknowledged_at ? formatDate(recipient.acknowledged_at) : null),
    fact("Seen", recipient.seen_at ? formatDate(recipient.seen_at) : null),
    fact("Last error", recipient.last_error),
  );
  return facts;
}

function modeControl(transfer, delivery, recipient, recipientId, options) {
  const modes = ["hosted", "replicated"].filter((mode) => modeAvailable(delivery, recipient, mode));
  if (!recipient.mode && !modes.length) return null;
  const draftKey = `${transfer.uid}:${recipientId}:mode`;
  const draft = drafts.get(draftKey) || { selected: recipient.mode, reviewed: false };
  if (draft.submittedMode && recipient.mode === draft.submittedMode) {
    draft.selected = recipient.mode;
    draft.reviewed = false;
    draft.submittedMode = null;
  }
  if (!draft.selected) draft.selected = recipient.mode;
  drafts.set(draftKey, draft);

  const section = el("section", "deliveryModeControl");
  const label = el("span", "deliveryControlLabel", "Delivery mode");
  const segments = el("div", "deliveryModeSegments");
  segments.setAttribute("role", "group");
  segments.setAttribute("aria-label", "Delivery mode");
  for (const mode of ["hosted", "replicated"]) {
    if (mode !== recipient.mode && !modes.includes(mode)) continue;
    const button = el("button", "deliveryModeButton", statusLabel(mode));
    button.type = "button";
    button.setAttribute("aria-pressed", String(draft.selected === mode));
    button.disabled = options.mutationsEnabled === false || mode === recipient.mode && modes.length === 0;
    button.addEventListener("click", () => {
      draft.selected = mode;
      draft.reviewed = false;
      rerender(options);
    });
    segments.append(button);
  }
  section.append(label, segments);
  if (draft.selected && draft.selected !== recipient.mode) {
    section.append(modeReview(transfer, delivery, recipient, recipientId, draft, options));
  }
  return section;
}

function modeReview(transfer, delivery, recipient, recipientId, draft, options) {
  const review = el("div", "deliveryModeReview");
  review.append(el("p", "", draft.selected === "replicated"
    ? "Replicated mode stores the recipient-redacted signed envelope on the recipient Cell. Later revocation stops future delivery but cannot erase evidence already received."
    : "Hosted mode keeps an authoritative remote reference and does not create a local executable commitment."));
  let check = null;
  if (draft.selected === "replicated") {
    const label = el("label", "deliveryReviewCheck");
    check = el("input");
    check.type = "checkbox";
    check.checked = draft.reviewed;
    check.disabled = options.mutationsEnabled === false;
    check.addEventListener("change", () => {
      draft.reviewed = check.checked;
      sync();
    });
    label.append(check, el("span", "", "I reviewed the exact disclosure and want this recipient to retain a redacted replica."));
    review.append(label);
  }

  const template = recipientAction(delivery, recipient, "set_mode", draft.selected);
  const key = `delivery:${transfer.uid}:${recipientId}:set_mode:${draft.selected}`;
  const submit = actionButton("Sign mode change", key, options, "primaryButton");
  const sync = () => {
    submit.disabled = options.mutationsEnabled === false
      || Boolean(options.actionState?.(key)?.busy || options.actionState?.(key)?.waiting)
      || !template || (draft.selected === "replicated" && !draft.reviewed);
  };
  sync();
  submit.addEventListener("click", () => {
    const action = withActionInput(template, {
      mode: draft.selected,
      recipient_person: recipient.person,
      recipient_organ: recipient.organ,
    });
    if (!action) return;
    draft.submittedMode = draft.selected;
    options.onAction?.(key, action);
  });
  review.append(submit);
  appendActionError(review, key, options);
  return review;
}

function deliveryActions(transfer, delivery, recipient, recipientId, options) {
  const root = el("div", "deliveryRecipientActions");
  for (const [name, label] of [["enqueue", "Queue delivery"], ["retry", "Retry delivery"]]) {
    if (!scopeCapability(delivery, recipient, name)) continue;
    const template = recipientAction(delivery, recipient, name);
    if (!template) continue;
    const key = `delivery:${transfer.uid}:${recipientId}:${name}`;
    const button = actionButton(label, key, options);
    button.addEventListener("click", () => {
      const action = withActionInput(template, {
        delivery: recipient.delivery_uid,
        recipient_person: recipient.person,
        recipient_organ: recipient.organ,
      });
      if (action) options.onAction?.(key, action);
    });
    root.append(button);
    appendActionError(root, key, options);
  }
  if (scopeCapability(delivery, recipient, "revoke")) {
    const revoke = revokeControl(transfer, delivery, recipient, recipientId, options);
    if (revoke) root.append(revoke);
  }
  return root;
}

function revokeControl(transfer, delivery, recipient, recipientId, options) {
  const template = recipientAction(delivery, recipient, "revoke");
  if (!template) return null;
  const draftKey = `${transfer.uid}:${recipientId}:revoke`;
  const draft = drafts.get(draftKey) || { reviewed: false };
  drafts.set(draftKey, draft);
  const group = el("div", "deliveryRevokeControl");
  const review = el("label", "deliveryReviewCheck");
  const check = el("input");
  check.type = "checkbox";
  check.checked = draft.reviewed;
  check.disabled = options.mutationsEnabled === false;
  check.addEventListener("change", () => {
    draft.reviewed = check.checked;
    sync();
  });
  review.append(check, el("span", "", "I understand revocation stops future access and delivery but does not erase signed evidence already received."));
  const key = `delivery:${transfer.uid}:${recipientId}:revoke`;
  const button = actionButton("Revoke future delivery", key, options, "dangerButton");
  const sync = () => {
    button.disabled = options.mutationsEnabled === false
      || Boolean(options.actionState?.(key)?.busy || options.actionState?.(key)?.waiting)
      || !draft.reviewed;
  };
  sync();
  button.addEventListener("click", () => {
    const action = withActionInput(template, {
      delivery: recipient.delivery_uid,
      recipient_person: recipient.person,
      recipient_organ: recipient.organ,
    });
    if (action) options.onAction?.(key, action);
  });
  group.append(review, button);
  appendActionError(group, key, options);
  return group;
}

function recipientAction(delivery, recipient, key, variant = null) {
  return projectedAction(recipient, key, variant) || projectedAction(delivery, key, variant);
}

function recipientOptionLabel(recipient) {
  const person = recipient.person_head || recipient.person_slug || recipient.person || "Person unavailable";
  const organ = recipient.organ_head || recipient.organ_slug || recipient.organ || "Organ unavailable";
  const contact = recipient.contact_state ? ` · ${statusLabel(recipient.contact_state)}` : "";
  return `${person} · ${organ}${contact}`;
}

function scopeCapability(delivery, recipient, ...keys) {
  return capability(recipient, ...keys) || capability(delivery, ...keys);
}

function modeAvailable(delivery, recipient, mode) {
  if (recipient.available_modes.length) return recipient.available_modes.includes(mode);
  return scopeCapability(delivery, recipient, `set_${mode}`, "set_mode", "set_delivery_mode");
}

function combinedBlockers(delivery, recipient) {
  return [
    ...blockers(recipient, "set_mode", "enqueue", "retry", "revoke"),
    ...blockers(delivery, "set_mode", "enqueue", "retry", "revoke"),
  ];
}

function actionButton(label, key, options, className = "secondaryButton") {
  const state = options.actionState?.(key);
  const button = el("button", className, state?.waiting ? "Awaiting live evidence" : state?.busy ? "Signing" : label);
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
  for (const value of values) list.append(el("li", "", statusLabel(value?.message || value?.code || value)));
  root.append(list);
}

function fact(label, value) {
  const item = el("div", "deliveryFact");
  item.append(el("dt", "", label), el("dd", "", value == null || value === "" ? "Unavailable" : String(value)));
  return item;
}

function rerender(options) {
  (options.onRefresh || options.onBulkSelectionChange)?.();
}
