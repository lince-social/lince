// A password-locked description (anicca/Secrets.md). Ciphertext is ordinary
// Record text everywhere else in Lince, so every surface that shows a body
// asks here first and shows the locked label instead of the envelope.
export const VAULT_MARKER = "lince-vault.v1";
export const LOCKED_LABEL = "🔒 Locked description";
export const PASSWORD_CHANGE_WARNING =
  "Older revisions of this Record keep the description they were saved with. " +
  "A new password protects the new one only — the old password still opens the old text.";

export function isLocked(text) {
  if (typeof text !== "string") return false;
  const fields = text.trim().split(" ");
  return fields.length === 5 && fields[0] === VAULT_MARKER;
}

async function call(path, payload) {
  const res = await fetch(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(text || "the vault refused");
  return text ? JSON.parse(text) : {};
}

// The password and the plaintext never become a Record Action, a Fact or a
// synced op: this is a local host call, and what comes back lives only in
// this page until it is locked again.
export async function unlockDescription(recordUid, password) {
  const out = await call("/host/vault/unlock", { record_uid: recordUid, password });
  return String(out.description || "");
}

export async function lockDescription(recordUid, password, description) {
  const payload = { record_uid: recordUid, password };
  if (typeof description === "string") payload.description = description;
  return call("/host/vault/lock", payload);
}
