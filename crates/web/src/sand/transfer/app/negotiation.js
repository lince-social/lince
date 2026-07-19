import { formatDate, invitationName, statusLabel } from "./model.js";

export function renderInvitationLifecycle(row, options) {
  const section = detailSection(`Invitations (${row.invitations.length})`, "invitations-section");
  const list = el("div", "invitationList");
  for (const invitation of row.invitations) list.append(invitationRow(row, invitation, options));
  if (!row.invitations.length) list.append(emptyInline("No invitations"));
  section.append(list);
  if (can(row, null, "address_invitation", "address")) section.append(addressForm(row, options));
  return section;
}

export function renderNegotiation(row, options) {
  if (!Array.isArray(row.threads)) return null;
  const section = detailSection("Negotiation", "negotiation-section");
  const threads = el("div", "threadList");
  for (const thread of row.threads) threads.append(threadBlock(row, thread, options));
  if (!row.threads.length) threads.append(emptyInline("No negotiation threads"));
  section.append(threads);
  if (can(row, null, "create_thread")) section.append(newThreadForm(row, options));
  return section;
}

export function canCounteroffer(row) {
  return can(row, null, "counteroffer", "counteroffer_terms");
}

export function canClaimOpen(row, promise) {
  const allowed = typeof promise?.capabilities?.claim === "boolean"
    ? promise.capabilities.claim
    : can(row, promise, "claim_open", "claim_open_promise");
  return allowed
    && promise.open === true
    && promise.state === "open";
}

function invitationRow(row, invitation, options) {
  const item = el("article", "invitationRow");
  const top = el("div", "invitationTop");
  const identity = el("div", "invitationIdentity");
  identity.append(
    el("strong", "personName", invitationName(invitation)),
    el("span", "invitationMeta", invitationMeta(invitation)),
  );
  top.append(identity, status(invitation.status));
  item.append(top);

  const events = invitationEvents(invitation);
  if (events.length) {
    const history = el("ol", "invitationEvents");
    for (const event of events) {
      const entry = el("li", "");
      entry.append(
        el("strong", "", statusLabel(event.kind || event.status || "updated")),
        el("span", "", [event.actor_head || event.actor_slug || event.actor_person || event.actor, event.at ? formatDate(event.at) : ""].filter(Boolean).join(" · ")),
      );
      history.append(entry);
    }
    item.append(history);
  }

  const actions = el("div", "inlineActions");
  addAction(actions, row, invitation, options, "accept", "Accept", {
    action: "accept-transfer-invitation",
    invitation: invitation.uid,
    transfer: row.uid,
    person: row.viewer?.person ?? invitation.addressed_person,
    expected_revision: row.revision,
    request_id: requestId("accept"),
  }, "accept_invitation", "accept");
  addAction(actions, row, invitation, options, "reject", "Reject", {
    action: "reject-transfer-invitation",
    invitation: invitation.uid,
    transfer: row.uid,
    person: row.viewer?.person ?? invitation.addressed_person,
    request_id: requestId("reject"),
  }, "reject_invitation", "reject");
  addAction(actions, row, invitation, options, "withdraw", "Withdraw", {
    action: "withdraw-transfer-invitation",
    invitation: invitation.uid,
    expected_revision: row.revision,
    request_id: requestId("withdraw"),
  }, "withdraw_invitation", "withdraw");
  if (can(row, invitation, "reopen_invitation", "reopen")) actions.append(reopenForm(row, invitation, options));
  if (actions.childElementCount) item.append(actions);
  const error = ["accept", "reject", "withdraw", "reopen"]
    .map((suffix) => actionState(options, invitationKey(invitation, suffix))?.error)
    .find(Boolean);
  if (error) item.append(alert(error));
  return item;
}

function addressForm(row, options) {
  const form = el("form", "inlineForm addressInvitationForm");
  const person = optionSelect(options.people || [], "Select Person");
  const expiry = dateTimeInput();
  const submit = button("Address invitation", "secondaryButton");
  const key = `transfer:${row.uid}:address`;
  syncButton(submit, actionState(options, key), "Address invitation", options);
  form.append(label("Person", person), label("Expires", expiry), submit);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!person.value || submit.disabled) return;
    options.onAction?.(key, {
      action: "address-transfer-invitation",
      transfer: row.uid,
      expected_revision: row.revision,
      request_id: requestId("address"),
      person: person.value,
      expires_at: isoOrNull(expiry.value),
    });
  });
  const error = actionState(options, key)?.error;
  if (error) form.append(alert(error));
  return form;
}

function reopenForm(row, invitation, options) {
  const form = el("form", "reopenForm");
  const expiry = dateTimeInput();
  const submit = button("Reopen", "secondaryButton");
  const key = invitationKey(invitation, "reopen");
  syncButton(submit, actionState(options, key), "Reopen", options);
  form.append(label("New expiry", expiry), submit);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (submit.disabled) return;
    options.onAction?.(key, {
      action: "reopen-transfer-invitation",
      invitation: invitation.uid,
      expected_revision: row.revision,
      request_id: requestId("reopen"),
      expires_at: isoOrNull(expiry.value),
    });
  });
  return form;
}

function addAction(root, row, invitation, options, suffix, text, action, ...capabilities) {
  if (!can(row, invitation, ...capabilities)) return;
  const key = invitationKey(invitation, suffix);
  const control = button(text, suffix === "reject" || suffix === "withdraw" ? "secondaryButton dangerButton" : "secondaryButton", "button");
  syncButton(control, actionState(options, key), text, options);
  control.addEventListener("click", () => options.onAction?.(key, action));
  root.append(control);
}

function threadBlock(row, thread, options) {
  const block = el("article", "threadBlock");
  block.append(el("h4", "", thread.head || "General"));
  const messages = el("div", "messageList");
  for (const message of Array.isArray(thread.messages) ? thread.messages : []) {
    const item = el("article", "messageRow");
    const meta = [message.sender, message.created_at ? formatDate(message.created_at) : ""].filter(Boolean).join(" · ");
    if (meta) item.append(el("div", "messageMeta", meta));
    item.append(el("p", "", message.body || ""));
    messages.append(item);
  }
  if (!messages.childElementCount) messages.append(emptyInline("No messages"));
  block.append(messages);
  if (can(row, thread, "create_message", "post_message")) block.append(messageForm(row, thread, options));
  return block;
}

function messageForm(row, thread, options) {
  const form = el("form", "messageForm");
  const body = el("textarea", "");
  body.rows = 3;
  body.placeholder = `Message ${thread.head || "thread"}`;
  const submit = button("Send", "primaryButton");
  const key = `transfer:${row.uid}:thread:${thread.uid}:message`;
  syncButton(submit, actionState(options, key), "Send", options);
  form.append(body, submit);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const text = body.value.trim();
    if (!text || submit.disabled) return;
    options.onAction?.(key, { action: "create-message", thread: thread.uid, body: text, parent: null });
  });
  const error = actionState(options, key)?.error;
  if (error) form.append(alert(error));
  return form;
}

function newThreadForm(row, options) {
  const form = el("form", "inlineForm newThreadForm");
  const head = el("input", "");
  head.type = "text";
  head.placeholder = "Thread title";
  const submit = button("New thread", "secondaryButton");
  const key = `transfer:${row.uid}:thread:create`;
  syncButton(submit, actionState(options, key), "New thread", options);
  form.append(head, submit);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const title = head.value.trim();
    if (!title || submit.disabled) return;
    options.onAction?.(key, { action: "create-thread", target: row.uid, head: title });
  });
  const error = actionState(options, key)?.error;
  if (error) form.append(alert(error));
  return form;
}

function invitationMeta(invitation) {
  const attempt = Number(invitation.attempt_number || invitation.attempt || 1);
  const expiry = invitation.expires_at ? `Expires ${formatDate(invitation.expires_at)}` : "No expiry";
  return `Attempt ${attempt} · ${expiry}`;
}

function invitationEvents(invitation) {
  if (Array.isArray(invitation.events)) return invitation.events;
  if (Array.isArray(invitation.lifecycle_events)) return invitation.lifecycle_events;
  if (Array.isArray(invitation.attempts)) {
    return invitation.attempts.flatMap((attempt) => Array.isArray(attempt.events) ? attempt.events : [attempt]);
  }
  return [];
}

function can(row, subject, ...names) {
  for (const name of names) {
    if (subject?.capabilities?.[name] === true) return true;
    if (row?.capabilities?.[name] === true) return true;
    const invitationCaps = row?.capabilities?.invitations;
    if (subject?.uid && invitationCaps?.[subject.uid]?.[name] === true) return true;
  }
  return false;
}

function actionState(options, key) { return options.actionState?.(key) || null; }
function invitationKey(invitation, suffix) { return `invitation:${invitation.uid}:${suffix}`; }

function syncButton(control, state, idleText, options) {
  control.disabled = options.mutationsEnabled === false || Boolean(state?.busy || state?.waiting);
  control.textContent = state?.waiting ? "Waiting for live update" : state?.busy ? "Signing" : idleText;
}

function requestId(kind) {
  if (globalThis.crypto?.randomUUID) return `transfer-${kind}:${globalThis.crypto.randomUUID()}`;
  return `transfer-${kind}:${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function isoOrNull(value) {
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

function dateTimeInput() {
  const control = el("input", "");
  control.type = "datetime-local";
  const now = new Date();
  control.min = new Date(now.getTime() - now.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
  return control;
}

function optionSelect(options, placeholder) {
  const control = el("select", "");
  control.append(new Option(placeholder, ""));
  for (const option of options) {
    const uid = String(option?.uid || "");
    if (!uid) continue;
    control.append(new Option(option?.head || option?.slug || uid, uid));
  }
  return control;
}

function label(text, control) {
  const wrapper = el("label", "inlineField");
  wrapper.append(el("span", "", text), control);
  return wrapper;
}

function status(value) {
  const node = el("span", "status", statusLabel(value));
  node.dataset.status = value || "pending";
  return node;
}

function detailSection(title, id) {
  const section = el("section", "detailSection");
  section.id = id;
  section.append(el("h3", "", title));
  return section;
}

function alert(text) {
  const node = el("div", "inlineAlert", text);
  node.setAttribute("role", "alert");
  return node;
}

function emptyInline(text) { return el("div", "emptyInline", text); }

function button(text, className, type = "submit") {
  const node = el("button", className, text);
  node.type = type;
  return node;
}

function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}
