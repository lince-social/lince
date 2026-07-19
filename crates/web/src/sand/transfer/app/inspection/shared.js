import { formatDate, formatQuantity, statusLabel } from "../model.js";

export { formatDate, formatQuantity, statusLabel };

export function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}

export function button(text, className, onClick) {
  const control = el("button", className, text);
  control.type = "button";
  control.addEventListener("click", onClick);
  return control;
}

export function empty(text) {
  return el("div", "emptyInline inspectionEmpty", text);
}

export function status(value) {
  const pill = el("span", "status", statusLabel(value || "unknown"));
  pill.dataset.status = value || "unknown";
  return pill;
}

export function labeledValue(label, value) {
  const item = el("div", "inspectionFact");
  item.append(el("dt", "", label), el("dd", "", present(value)));
  return item;
}

export function present(value) {
  if (value == null || value === "") return "Unavailable";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

export function projectedArray(value, ...keys) {
  if (Array.isArray(value)) return value;
  for (const key of keys) if (Array.isArray(value?.[key])) return value[key];
  return [];
}

export function proofState(value) {
  const raw = typeof value === "object" && value
    ? value.state || value.mechanism || value.kind || value.status : value;
  const state = String(raw || "missing").toLowerCase().replaceAll("-", "_");
  if (["direct_fact_signature", "fact_signature", "signed_fact", "direct"].includes(state)) {
    return { key: "direct_fact_signature", label: "Direct Fact signature" };
  }
  if (["verified_action_intent", "signed_action_intent", "action_intent", "verified_intent"].includes(state)) {
    return { key: "verified_action_intent", label: "Verified signed Action intent" };
  }
  if (["unsigned", "unsigned_system", "unsigned_local", "system", "local"].includes(state)) {
    return { key: "unsigned", label: "Unsigned system or local evidence" };
  }
  if (["invalid", "invalid_signature", "invalid_proof"].includes(state)) {
    return { key: "invalid", label: "Invalid proof" };
  }
  return { key: "missing", label: "Proof unavailable" };
}

export function compactId(value) {
  const text = String(value || "");
  return text.length > 22 ? `${text.slice(0, 10)}...${text.slice(-8)}` : text || "Unavailable";
}
