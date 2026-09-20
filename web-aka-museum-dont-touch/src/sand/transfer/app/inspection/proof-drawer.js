import { button, compactId, el, empty, formatDate, labeledValue, present, projectedArray, proofState, status } from "./shared.js";

export function createProofDrawer(row, timeline) {
  const entries = proofEntries(row, timeline.items);
  const titleId = `transfer-proof-title-${safeId(row.uid)}`;
  const dialogId = `transfer-proof-${safeId(row.uid)}`;
  const dialog = el("dialog", "proofDrawer");
  dialog.id = dialogId;
  dialog.setAttribute("aria-labelledby", titleId);
  const shell = el("div", "proofDrawerShell");
  const header = el("header", "proofDrawerHeader");
  const identity = el("div", "proofDrawerIdentity");
  identity.append(el("span", "eyebrow", "Transfer evidence"), el("h3", "", "Proof and source Facts"));
  identity.querySelector("h3").id = titleId;
  const close = button("Close", "secondaryButton proofDrawerClose", () => closeDialog(dialog));
  header.append(identity, close);
  const body = el("div", "proofDrawerBody");
  const list = el("nav", "proofIndex");
  list.setAttribute("aria-label", "Source Fact index");
  const detail = el("section", "proofDetail");
  let selected = null;

  const renderSelected = () => {
    detail.replaceChildren();
    if (!selected) {
      detail.append(empty("No proof selected"));
      return;
    }
    const heading = el("header", "proofDetailHeading");
    heading.append(el("div", "", selected.title), status(selected.proof.key));
    detail.append(heading);
    const facts = el("dl", "proofFactGrid");
    facts.append(
      labeledValue("Proof state", selected.proof.label),
      labeledValue("Fact", selected.fact),
      labeledValue("Target", selected.targetUid),
      labeledValue("Person author", selected.person),
      labeledValue("Occurred", selected.occurredAt ? formatDate(selected.occurredAt) : null),
      labeledValue("Request ID", selected.requestId),
      labeledValue("Revision", selected.revision),
      labeledValue("Fact hash", selected.hash),
      labeledValue("Previous hash", selected.previousHash),
      labeledValue("Authoritative", selected.authoritative),
    );
    detail.append(facts);
    if (selected.signature) {
      const signature = el("section", "proofMaterial");
      signature.append(el("strong", "", "Projected signature material"), el("code", "", present(selected.signature)));
      detail.append(signature);
    }
    if (selected.intent) {
      const intent = el("section", "proofMaterial");
      intent.append(el("strong", "", "Signed Action intent"), el("code", "", present(selected.intent)));
      detail.append(intent);
    }
    const links = projectedArray(selected.links, "items");
    if (links.length) {
      const linked = el("section", "proofLinks");
      linked.append(el("strong", "", "Linked evidence"));
      for (const link of links) linked.append(el("span", "", present(link)));
      detail.append(linked);
    }
    const raw = el("details", "proofRaw");
    raw.append(el("summary", "", "Projected proof fields"), el("pre", "", JSON.stringify(selected.raw, null, 2)));
    detail.append(raw);
  };

  for (const entry of entries) {
    const control = button("", "proofIndexItem", () => {
      selected = entry;
      for (const candidate of list.querySelectorAll(".proofIndexItem")) {
        candidate.setAttribute("aria-current", String(candidate === control));
      }
      renderSelected();
    });
    control.append(
      el("strong", "", entry.title),
      el("span", "", entry.fact ? `Fact ${compactId(entry.fact)}` : entry.proof.label),
    );
    list.append(control);
  }
  if (!entries.length) list.append(empty("No source Facts are visible in this projection."));
  body.append(list, detail);
  shell.append(header, body);
  dialog.append(shell);
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) closeDialog(dialog);
  });
  dialog.addEventListener("keydown", (event) => {
    if (event.key === "Escape") closeDialog(dialog);
  });
  dialog.addEventListener("close", () => dialog.returnFocus?.focus());
  renderSelected();

  const trigger = button("Proof", "secondaryButton inspectionProofTrigger", () => {
    dialog.returnFocus = trigger;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
    const first = list.querySelector(".proofIndexItem");
    if (first) {
      first.click();
      first.focus();
    }
    else close.focus();
  });
  trigger.setAttribute("aria-haspopup", "dialog");
  trigger.setAttribute("aria-controls", dialogId);

  return {
    dialog,
    entries,
    openFor(item) {
      trigger.click();
      const match = entries.find((entry) => (item.fact && entry.fact === item.fact) || entry.uid === item.uid);
      if (!match) return;
      const index = entries.indexOf(match);
      const control = list.querySelectorAll(".proofIndexItem")[index];
      control?.click();
      control?.focus();
    },
    trigger,
  };
}

function proofEntries(row, timelineItems) {
  const singularProof = row.proof && typeof row.proof === "object" && !Array.isArray(row.proof)
    && !projectedArray(row.proof, "items", "facts", "evidence").length;
  const projected = Array.isArray(row.proof) ? row.proof
    : row.proof && typeof row.proof === "object"
      ? projectedArray(row.proof, "items", "facts", "evidence").length
        ? projectedArray(row.proof, "items", "facts", "evidence") : [row.proof]
      : [];
  const entries = projected.map((item, index) => normalizeProof(
    singularProof ? { title: "Current revision proof", ...item } : item,
    index,
  ));
  for (const timeline of timelineItems) {
    if (!timeline.fact && !timeline.proofRaw) continue;
    entries.push(normalizeProof({
      ...timeline.raw,
      uid: timeline.uid,
      title: timeline.title,
      target_uid: timeline.targetUid,
      occurred_at: timeline.occurredAt,
      person: timeline.person,
      fact: timeline.fact,
      request_id: timeline.requestId,
      revision: timeline.revision,
      proof: timeline.proofRaw,
      links: timeline.links,
    }, entries.length));
  }
  const current = row.revision_evidence?.current;
  if (current?.fact) entries.push(normalizeProof({
    ...current,
    uid: `revision-proof:${current.fact}`,
    title: `Revision ${current.revision}`,
    target_uid: row.uid,
    proof_state: current.signature ? "direct_fact_signature" : current.proof_state,
  }, entries.length));
  const seen = new Set();
  return entries.filter((entry) => {
    const key = entry.fact || entry.uid;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function normalizeProof(raw, index) {
  const item = raw && typeof raw === "object" ? raw : {};
  const factObject = item.fact && typeof item.fact === "object" ? item.fact : {};
  const proofRaw = item.proof || item.proof_state || item.mechanism || item.state || factObject.proof || null;
  const proofObject = proofRaw && typeof proofRaw === "object" ? proofRaw : {};
  const signature = item.fact_signature || item.signature || factObject.signature
    || proofObject.fact_signature || proofObject.signature;
  const derivedState = item.mechanism || item.state || proofRaw || (signature ? "direct_fact_signature" : "missing");
  return {
    raw: item,
    uid: String(item.uid || item.event_uid || factObject.uid || item.fact || `proof-${index}`),
    title: String(item.title || item.label || item.kind || item.type || "Evidence"),
    proof: proofState(derivedState),
    fact: factObject.uid || item.fact_uid || (typeof item.fact === "string" ? item.fact : null),
    targetUid: item.target?.uid || item.target_uid || item.record || item.transfer || proofObject.record || null,
    person: item.person || item.author_person || item.actor || item.actor_person || factObject.actor || proofObject.actor || null,
    occurredAt: item.occurred_at || item.at || item.created_at || factObject.at || proofObject.at || null,
    requestId: item.request_id || item.idempotency_key || null,
    revision: item.revision ?? null,
    signature,
    intent: item.action_intent || item.intent || proofObject.action_intent || null,
    hash: item.hash || factObject.hash || proofObject.hash || null,
    previousHash: item.previous_hash || factObject.previous_hash || proofObject.previous_hash || null,
    authoritative: item.authoritative ?? proofObject.authoritative,
    links: item.links || item.linked_ids || [],
  };
}

function closeDialog(dialog) {
  if (typeof dialog.close === "function" && dialog.open) dialog.close();
  else {
    dialog.removeAttribute("open");
    dialog.returnFocus?.focus();
  }
}

function safeId(value) {
  return String(value || "transfer").replaceAll(/[^a-zA-Z0-9_-]/g, "-");
}
