import { renderDeliveryConflicts } from "./delivery/conflicts.js";
import { capability, projectedAction, withActionInput } from "./delivery/model.js";
import { renderDeliveryRecipients } from "./delivery/recipients.js";
import { renderPackageReceipts } from "./delivery/receipts.js";
import { compactId, el, formatDate, status, statusLabel } from "./inspection/shared.js";

export function renderSocialDelivery(row, options) {
  const delivery = row.social_delivery;
  if (!delivery) return null;
  const section = el("section", "inspectionBand socialDeliveryBand");
  const heading = el("header", "inspectionBandHeading socialDeliveryHeading");
  const identity = el("div", "socialDeliveryIdentity");
  identity.append(
    el("div", "eyebrow", "Cell-to-Cell"),
    el("h4", "", "Social delivery"),
  );
  const headingState = el("div", "socialDeliveryHeadingState");
  if (delivery.mode) headingState.append(status(delivery.mode));
  headingState.append(status(delivery.freshness.state || delivery.state || "unknown"));
  heading.append(identity, headingState);
  section.append(heading, authorityFacts(delivery));

  const refresh = refreshControl(row, delivery, options);
  if (refresh) section.append(refresh);
  const executor = executorControl(row, delivery, options);
  if (executor) section.append(executor);
  section.append(
    renderDeliveryRecipients(row, delivery, options),
    renderApplicationHandoffs(row, delivery, options),
    renderPackageReceipts(delivery),
    renderDeliveryConflicts(row, delivery, options),
  );
  const history = replicaHistory(delivery);
  if (history) section.append(history);
  return section;
}

function renderApplicationHandoffs(transfer, delivery, options) {
  const handoffs = Array.isArray(delivery.application_handoffs) ? delivery.application_handoffs : [];
  const section = el("section", "deliveryEvidenceGroup");
  const heading = el("header", "deliveryGroupHeading");
  heading.append(el("strong", "", "Settlement applications"), el("span", "", `${handoffs.length} handoff${handoffs.length === 1 ? "" : "s"}`));
  section.append(heading);
  if (!handoffs.length) {
    section.append(el("p", "emptyState", "No cross-Cell settlement applications"));
    return section;
  }
  for (const handoff of handoffs) {
    const row = el("article", "deliveryConflict");
    row.append(el("strong", "", `Slice ${handoff.canonical_quantity ?? "?"}`), status(handoff.state || "pending"));
    const optionsList = Array.isArray(handoff.local_record_options) ? handoff.local_record_options : [];
    if (capability(handoff, "apply")) {
      const select = el("select", "");
      select.append(new Option("Select private local Record", ""));
      for (const record of optionsList) select.append(new Option(`${record.head || compactId(record.uid)} (${record.quantity})`, record.uid));
      const key = `delivery:${transfer.uid}:handoff:${handoff.uid}:apply`;
      const button = el("button", "primaryButton", "Apply privately");
      button.type = "button";
      button.disabled = options.mutationsEnabled === false;
      button.addEventListener("click", () => {
        const action = withActionInput(projectedAction(handoff, "apply"), { local_record: select.value });
        if (action && select.value) options.onAction?.(key, action);
      });
      row.append(select, button);
      const error = options.actionState?.(key)?.error;
      if (error) row.append(el("div", "inlineAlert", error));
    }
    section.append(row);
  }
  return section;
}

function authorityFacts(delivery) {
  const facts = el("dl", "socialDeliveryFacts");
  facts.append(
    fact("Authority", delivery.authority_organ ? compactId(delivery.authority_organ) : null),
    fact("This view", delivery.authority_role ? statusLabel(delivery.authority_role) : null),
    fact("Canonical writes", delivery.canonical_writes ? statusLabel(delivery.canonical_writes) : null),
    fact("Local Organ", delivery.local_organ ? compactId(delivery.local_organ) : null),
    fact("Fresh at", delivery.freshness.projected_at ? formatDate(delivery.freshness.projected_at) : null),
    fact("Cursor", cursorLabel(delivery.freshness)),
  );
  if (delivery.freshness.last_error) {
    const error = el("div", "socialDeliveryFreshnessError", delivery.freshness.last_error);
    error.setAttribute("role", "status");
    facts.append(error);
  }
  return facts;
}

function refreshControl(transfer, delivery, options) {
  if (!capability(delivery, "refresh", "pull")) return null;
  const template = projectedAction(delivery, "refresh") || projectedAction(delivery, "pull");
  if (!template) return null;
  const key = `delivery:${transfer.uid}:refresh`;
  const state = options.actionState?.(key);
  const block = el("div", "socialDeliveryRefresh");
  const copy = el("div", "socialDeliveryRefreshCopy");
  copy.append(
    el("strong", "", "Authoritative refresh"),
    el("span", "", delivery.freshness.last_pull_at
      ? `Last pull ${formatDate(delivery.freshness.last_pull_at)}` : "No completed pull is projected"),
  );
  const button = el("button", "secondaryButton", state?.waiting ? "Awaiting live evidence" : state?.busy ? "Refreshing" : "Refresh");
  button.type = "button";
  button.disabled = options.mutationsEnabled === false || Boolean(state?.busy || state?.waiting);
  button.addEventListener("click", () => {
    const action = withActionInput(template);
    if (action) options.onAction?.(key, action);
  });
  block.append(copy, button);
  if (state?.error) {
    const alert = el("div", "inlineAlert", state.error);
    alert.setAttribute("role", "alert");
    block.append(alert);
  }
  return block;
}

/**
 * Which of YOUR Cells retries this Transfer's deliveries (Ontology C7).
 *
 * Every word here is about your own machines. Delivery retries are the one
 * non-Rule scheduler that reaches outward — two Cells draining the same outbox
 * send the recipient the same envelope twice — so this names one of them and
 * the others stand down. It changes nothing the recipient agreed to, which is
 * why it sits beside the refresh control rather than among the recipients.
 *
 * The button names THIS Cell and no other. It is the only uid this page can be
 * sure of, and pinning delivery to a machine you are not sitting at is how a
 * Transfer ends up waiting on a laptop that is closed. Moving it means opening
 * the other Cell and pressing it there — the manual takeover the design chose
 * over a heartbeat that would hand delivery to whichever Cell merely cannot see
 * the current one.
 */
function executorControl(transfer, delivery, options) {
  if (!capability(delivery, "designate_executor")) return null;
  const template = projectedAction(delivery, "designate_executor");
  if (!template) return null;
  const executor = delivery.executor && typeof delivery.executor === "object"
    ? delivery.executor
    : {};
  const designated = executor.designated_cell || null;
  const thisCell = executor.this_cell || null;
  const key = `delivery:${transfer.uid}:executor`;
  const state = options.actionState?.(key);
  const block = el("div", "socialDeliveryRefresh");
  const copy = el("div", "socialDeliveryRefreshCopy");
  copy.append(
    el("strong", "", "Delivering Cell"),
    el("span", "", !designated
      ? "Every Cell holding this Transfer retries it"
      : designated === thisCell
        ? "This Cell delivers it; your other Cells stand down"
        : `Another Cell delivers it (${compactId(designated)})`),
  );
  const clearing = Boolean(designated);
  const button = el(
    "button",
    "secondaryButton",
    state?.busy ? "Saving" : clearing ? "Let any Cell deliver" : "Only this Cell",
  );
  button.type = "button";
  // Without a uid for this Cell, "Only this Cell" would send a null and CLEAR
  // the designation — a button doing the opposite of its label.
  button.disabled = options.mutationsEnabled === false
    || Boolean(state?.busy)
    || (!clearing && !thisCell);
  button.addEventListener("click", () => {
    options.onAction?.(key, { ...template, cell_uid: clearing ? null : thisCell });
  });
  block.append(copy, button);
  if (!clearing && !thisCell) {
    block.append(el("span", "", "This Cell cannot identify itself, so it cannot be designated."));
  }
  if (state?.error) {
    const alert = el("div", "inlineAlert", state.error);
    alert.setAttribute("role", "alert");
    block.append(alert);
  }
  return block;
}

function replicaHistory(delivery) {
  if (!delivery.replica_history.length) return null;
  const section = el("section", "deliveryEvidenceGroup replicaHistory");
  const heading = el("header", "deliveryGroupHeading");
  heading.append(el("strong", "", "Replica history"), el("span", "", `${delivery.replica_history.length} retained`));
  const list = el("ol", "replicaHistoryList");
  for (const raw of delivery.replica_history) {
    const item = raw && typeof raw === "object" ? raw : { revision: raw };
    const row = el("li", "replicaHistoryRow");
    row.append(
      el("strong", "", item.revision == null ? "Revision unavailable" : `Revision ${item.revision}`),
      el("span", "", [
        item.cursor != null && `Cursor ${item.cursor}`,
        item.at && formatDate(item.at),
        item.envelope && `Envelope ${compactId(item.envelope)}`,
      ].filter(Boolean).join(" · ")),
    );
    list.append(row);
  }
  section.append(heading, list);
  return section;
}

function fact(label, value) {
  const item = el("div", "deliveryFact");
  item.append(el("dt", "", label), el("dd", "", value == null || value === "" ? "Unavailable" : String(value)));
  return item;
}

function cursorLabel(freshness) {
  if (freshness.cursor == null && freshness.origin_cursor == null) return null;
  if (freshness.origin_cursor == null) return String(freshness.cursor);
  return `${freshness.cursor ?? "?"} / ${freshness.origin_cursor}`;
}
