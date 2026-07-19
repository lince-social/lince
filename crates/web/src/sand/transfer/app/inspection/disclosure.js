import { button, el, empty, present, projectedArray, status, statusLabel } from "./shared.js";

export function renderDisclosureAndThreads(row) {
  const section = el("section", "inspectionBand disclosureBand");
  section.append(el("header", "inspectionBandHeading", "Visibility and conversation"));
  const disclosure = disclosureProjection(row);
  section.append(renderDisclosure(row, disclosure), renderThreadIndex(row));
  return section;
}

function disclosureProjection(row) {
  const inspection = row.inspection && typeof row.inspection === "object" ? row.inspection : {};
  const projection = row.visibility_projection || row.disclosure || inspection.disclosure || null;
  return projection && typeof projection === "object" ? projection : null;
}

function renderDisclosure(row, projection) {
  const block = el("section", "disclosurePreview");
  const heading = el("header", "disclosureHeading");
  heading.append(el("strong", "", "Exact disclosure preview"), status(row.visibility || "hidden"));
  block.append(heading);
  if (!projection) {
    block.append(empty("Exact authorized recipients and disclosed fields are not available in this projection."));
    return block;
  }

  const policy = el("dl", "disclosurePolicy");
  policy.append(
    disclosureFact("Policy", projection.policy || row.visibility),
    disclosureFact("Scope", projection.scope),
    disclosureFact("Maximum proximity", projection.max_proximity),
    disclosureFact("Field overrides", projection.disclosure?.field_overrides_supported),
  );
  block.append(policy);

  const recipients = projectedArray(projection.recipients, "items", "people", "organs");
  if (recipients.length) {
    const recipientBlock = el("section", "disclosureRecipients");
    recipientBlock.append(el("strong", "", "Authorized recipients"));
    const list = el("ul", "disclosureRecipientList");
    for (const raw of recipients) {
      const recipient = raw && typeof raw === "object" ? raw : { uid: raw };
      const item = el("li", "disclosureRecipient");
      item.append(
        el("span", "", recipient.head || recipient.name || recipient.slug || recipient.uid || "Recipient"),
        el("small", "", [recipient.kind, recipient.reason, recipient.state, recipient.scope, recipient.proximity != null && `proximity ${recipient.proximity}`].filter(Boolean).map(statusLabel).join(" · ")),
      );
      list.append(item);
    }
    recipientBlock.append(list);
    block.append(recipientBlock);
  }

  const rules = projectedArray(projection.explicit_rules, "items", "rules");
  if (rules.length) {
    const ruleBlock = el("section", "disclosureRules");
    ruleBlock.append(el("strong", "", "Explicit visibility rules"));
    const list = el("ul", "disclosureRuleList");
    for (const rule of rules) {
      list.append(el("li", "", [
        rule.subject_kind && statusLabel(rule.subject_kind),
        rule.subject_uid,
        rule.field && statusLabel(rule.field),
        rule.grant_level && statusLabel(rule.grant_level),
      ].filter(Boolean).join(" · ")));
    }
    ruleBlock.append(list);
    block.append(ruleBlock);
  }

  const groups = disclosureGroups(projection);
  const groupList = el("div", "disclosureGroups");
  for (const group of groups) groupList.append(disclosureGroup(group));
  if (groupList.childElementCount) block.append(groupList);
  else block.append(empty("No disclosed field groups are visible for this viewer."));
  return block;
}

function disclosureGroups(projection) {
  const groups = projectedArray(projection.groups, "items", "sections");
  if (groups.length) return groups;
  const exact = projection.disclosure && typeof projection.disclosure === "object"
    ? projection.disclosure : null;
  if (exact) {
    const included = projectedArray(exact.included, "items", "fields");
    const excluded = projectedArray(exact.excluded, "items", "fields");
    return [
      included.length && { name: "Included fields", fields: included.map(disclosureField) },
      excluded.length && { name: "Excluded fields", fields: excluded.map((field) => ({ ...disclosureField(field), value: "Excluded" })) },
    ].filter(Boolean);
  }
  const fields = projection.fields;
  if (!fields || typeof fields !== "object" || Array.isArray(fields)) return [];
  return Object.entries(fields).map(([name, value]) => ({ name, fields: value }));
}

function disclosureField(raw) {
  if (raw && typeof raw === "object") return raw;
  return { name: raw, value: "Included" };
}

function disclosureGroup(raw) {
  const group = raw && typeof raw === "object" ? raw : { name: raw };
  const section = el("section", "disclosureGroup");
  const heading = el("header", "disclosureGroupHeading");
  heading.append(
    el("strong", "", group.label || group.name || group.kind || "Disclosed fields"),
    group.audience || group.scope ? el("span", "", statusLabel(group.audience || group.scope)) : document.createTextNode(""),
  );
  section.append(heading);
  const fields = Array.isArray(group.fields) ? group.fields
    : group.fields && typeof group.fields === "object"
      ? Object.entries(group.fields).map(([name, value]) => ({ name, value })) : [];
  const list = el("dl", "disclosureFieldList");
  for (const rawField of fields) {
    const field = rawField && typeof rawField === "object" ? rawField : { name: rawField };
    if (field.disclosed === false || field.visible === false) continue;
    const item = el("div", "disclosureField");
    item.append(
      el("dt", "", field.label || field.name || field.path || "Field"),
      el("dd", "", present(field.value ?? field.preview ?? field.type ?? "Included")),
    );
    list.append(item);
  }
  if (list.childElementCount) section.append(list);
  return section;
}

function disclosureFact(label, value) {
  const item = el("div", "disclosureFact");
  item.append(el("dt", "", label), el("dd", "", present(value)));
  return item;
}

function renderThreadIndex(row) {
  const block = el("section", "threadIndex");
  const threads = Array.isArray(row.threads) ? row.threads : [];
  const heading = el("header", "threadIndexHeading");
  heading.append(el("strong", "", "Threads"), el("span", "", `${threads.length} visible`));
  block.append(heading);
  if (!threads.length) {
    block.append(empty("No visible negotiation threads"));
    return block;
  }
  const list = el("ul", "threadIndexList");
  for (const thread of threads) {
    const messages = Array.isArray(thread.messages) ? thread.messages : [];
    const latest = messages[messages.length - 1];
    const item = el("li", "threadIndexRow");
    const identity = el("div", "threadIndexIdentity");
    identity.append(
      el("strong", "", thread.head || "General"),
      el("span", "", latest?.body ? latest.body : `${messages.length} message${messages.length === 1 ? "" : "s"}`),
    );
    const open = button("Open", "textButton", () => focusThreads());
    open.setAttribute("aria-label", `Open thread ${thread.head || "General"}`);
    item.append(identity, open);
    list.append(item);
  }
  block.append(list);
  return block;
}

function focusThreads() {
  const target = document.getElementById("negotiation-section");
  if (!target) return;
  target.tabIndex = -1;
  target.scrollIntoView({ behavior: "smooth", block: "start" });
  target.focus({ preventScroll: true });
}
