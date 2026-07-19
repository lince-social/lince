import { renderAccountingAndNavigation } from "./inspection/accounting.js";
import { renderCorrections } from "./inspection/corrections.js";
import { renderDisclosureAndThreads } from "./inspection/disclosure.js";
import { renderSocialDelivery } from "./delivery.js";
import { createProofDrawer } from "./inspection/proof-drawer.js";
import { el } from "./inspection/shared.js";
import { renderTimeline, timelineProjection } from "./inspection/timeline.js";

export function renderDetailEvidence(row, rows, options) {
  const timeline = timelineProjection(row);
  const proof = createProofDrawer(row, timeline);
  const refresh = options.onRefresh || options.onBulkSelectionChange;
  const section = el("section", "detailSection inspectionWorkspace");
  section.id = "inspection-section";
  const header = el("header", "inspectionHeader");
  const identity = el("div", "inspectionIdentity");
  identity.append(el("h3", "", "Evidence and records"));
  header.append(identity, proof.trigger);
  section.append(header, renderTimeline(row, timeline, proof.openFor, refresh));
  const delivery = renderSocialDelivery(row, options);
  if (delivery) section.append(delivery);
  const corrections = renderCorrections(row, rows, options);
  if (corrections) section.append(corrections);
  section.append(
    renderAccountingAndNavigation(row, rows, options),
    renderDisclosureAndThreads(row),
    proof.dialog,
  );
  return section;
}
