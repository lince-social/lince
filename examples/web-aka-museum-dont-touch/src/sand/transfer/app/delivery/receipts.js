import { compactId, el, empty, formatDate, proofState, status, statusLabel } from "../inspection/shared.js";

export function renderPackageReceipts(delivery) {
  const section = el("section", "deliveryEvidenceGroup packageReceiptGroup");
  const heading = el("header", "deliveryGroupHeading");
  heading.append(
    el("strong", "", "Network package receipts"),
    el("span", "", `${delivery.package_receipts.length} event${delivery.package_receipts.length === 1 ? "" : "s"}`),
  );
  section.append(
    heading,
    el("p", "deliverySeparationNote", "These prove envelope receipt or viewing only. They do not confirm real-world delivery, receipt, or settlement."),
  );
  if (!delivery.package_receipts.length) {
    section.append(empty("No package receipt or seen evidence"));
    return section;
  }
  const list = el("ol", "packageReceiptList");
  for (const receipt of delivery.package_receipts) list.append(receiptRow(receipt));
  section.append(list);
  return section;
}

function receiptRow(receipt) {
  const item = el("li", "packageReceiptRow");
  const identity = el("div", "packageReceiptIdentity");
  const kind = receipt.kind || "package_received";
  identity.append(
    el("strong", "", statusLabel(kind)),
    el("span", "", [
      receipt.at ? formatDate(receipt.at) : "Time unavailable",
      receipt.organ && `Organ ${compactId(receipt.organ)}`,
    ].filter(Boolean).join(" · ")),
  );
  const references = el("div", "packageReceiptReferences");
  references.append(
    el("span", "", receipt.envelope ? `Envelope ${compactId(receipt.envelope)}` : "Envelope unavailable"),
    el("span", "", receipt.cursor == null ? "Cursor unavailable" : `Cursor ${receipt.cursor}`),
  );
  const proof = proofState(receipt.proof ?? receipt.proof_state ?? receipt.signature);
  item.append(identity, references, status(proof.key));
  return item;
}
