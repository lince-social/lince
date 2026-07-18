#!/usr/bin/env bash
# Behavioral verification of the Roles & Permissions sand (2026-07-18): the
# CRUD surface for the permission/role/user system, driven entirely through
# Protein (source: "auth") + Actions (create-role, create-user, assign-role,
# grant-permission, revoke-permission) — the engine enforces every action
# against role:create/user:create/user:assign_role/permission:assign; this
# sand adds no second enforcement layer, so the test is about the wiring:
#   1. subscribes to { source: "auth" } and renders roles/users/catalog
#   2. a role's permission checkboxes reflect what's granted
#   3. toggling a checkbox sends grant-permission / revoke-permission
#   4. a user's role <select> sends assign-role on change
#   5. "+ New role" / "+ New user" send create-role / create-user
#   6. every mutating action re-subscribes (refresh) to pick up the new state
#   7. a rejected act() (Forbidden) shows the error and reverts an optimistic toggle
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/permissions-sand-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SRC="$ROOT/crates/web/src/sand/permissions/permissions.html"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Extract everything between <body> and </body> (the sand's own frame.js
# include lives in <head>, so this naturally excludes it — LinceWidgetHost is
# stubbed directly in the harness instead).
awk '/<body>/{f=1; next} /<\/body>/{f=0} f' "$SRC" > "$WORK/body.html"
[ -s "$WORK/body.html" ] || { echo "could not extract permissions.html body"; exit 1; }
grep -q "Roles &amp; Permissions" "$SRC" || { echo "permissions.html not found"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<script>
  window.__acts = [];
  window.__subCount = 0;
  window.__failNext = false;
  window.__rows = [
    { kind: "role", id: "1", name: "admin", permissions: ["record:create", "record:delete"] },
    { kind: "role", id: "2", name: "support", permissions: ["record:read"] },
    { kind: "user", id: "1", username: "root", name: "Root", role: "admin" },
    { kind: "user", id: "2", username: "amy", name: "Amy", role: "support" },
    { kind: "permission_catalog", keys: ["record:create", "record:read", "record:delete", "record:delete_own"] },
  ];
  window.LinceWidgetHost = {
    onLive(handler) { handler(true); return () => {}; },
    subscribeProtein(subId, protein, handler) {
      window.__subCount++;
      window.__lastProtein = protein;
      handler({ rows: window.__rows });
      return () => {};
    },
    act(action) {
      window.__acts.push(action);
      if (window.__failNext) {
        window.__failNext = false;
        return Promise.reject(new Error("forbidden: missing permission:assign permission"));
      }
      return Promise.resolve({ created: "9", facts: [], warnings: [] });
    },
  };
</script>
HTML
cat "$WORK/body.html" >> "$WORK/harness.html"
cat >> "$WORK/harness.html" <<'HTML'
<script>
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const d = document;

  (async () => {
    const results = {};
    const mark = () => { document.title = "RESULT=" + JSON.stringify(results); };
    try {
      await wait(50);

      results.subscribes_auth_source = window.__lastProtein && window.__lastProtein.source === "auth";

      const adminCard = d.querySelector('.role[data-role="admin"]');
      const supportCard = d.querySelector('.role[data-role="support"]');
      results.role_cards_render = !!adminCard && !!supportCard;

      const adminCreateBox = adminCard.querySelector('.grant[data-permission="record:create"] input');
      const adminReadBox = adminCard.querySelector('.grant[data-permission="record:read"] input');
      results.grant_checkbox_reflects_state = !!adminCreateBox && adminCreateBox.checked
        && !!adminReadBox && !adminReadBox.checked;

      // Toggle an ungranted permission ON for admin -> grant-permission.
      window.__acts.length = 0;
      const before = window.__subCount;
      adminReadBox.checked = true;
      adminReadBox.dispatchEvent(new Event("change"));
      await wait(50);
      results.checkbox_check_sends_grant = window.__acts.some((a) => a
        && a.action === "grant-permission" && a.role === "admin" && a.permission === "record:read");
      results.mutating_action_refreshes = window.__subCount > before;

      // Toggle a granted permission OFF for admin -> revoke-permission.
      window.__acts.length = 0;
      adminCreateBox.checked = false;
      adminCreateBox.dispatchEvent(new Event("change"));
      await wait(50);
      results.checkbox_uncheck_sends_revoke = window.__acts.some((a) => a
        && a.action === "revoke-permission" && a.role === "admin" && a.permission === "record:create");

      // Users table + role reassignment.
      const amyRow = d.querySelector('tr[data-user="2"]');
      results.user_row_renders = !!amyRow && amyRow.textContent.includes("amy") && amyRow.textContent.includes("Amy");
      const roleSelect = amyRow.querySelector("select");
      results.user_role_select_shows_current = roleSelect.value === "support";
      window.__acts.length = 0;
      roleSelect.value = "admin";
      roleSelect.dispatchEvent(new Event("change"));
      await wait(50);
      results.role_select_change_sends_assign_role = window.__acts.some((a) => a
        && a.action === "assign-role" && a.user === "2" && a.role === "admin");

      // Create role.
      window.__acts.length = 0;
      d.getElementById("role-name").value = "auditor";
      d.getElementById("role-create").click();
      await wait(50);
      results.create_role_action = window.__acts.some((a) => a
        && a.action === "create-role" && a.name === "auditor");

      // Create user.
      window.__acts.length = 0;
      d.getElementById("u-username").value = "ben";
      d.getElementById("u-name").value = "Ben";
      d.getElementById("u-password").value = "hunter2";
      d.getElementById("u-role").value = "support";
      d.getElementById("u-create").click();
      await wait(50);
      results.create_user_action = window.__acts.some((a) => a
        && a.action === "create-user" && a.username === "ben" && a.name === "Ben"
        && a.password === "hunter2" && a.role === "support");

      // A rejected act() (Forbidden) surfaces the error and reverts an optimistic toggle.
      window.__failNext = true;
      const box = supportCard.querySelector('.grant[data-permission="record:create"] input');
      box.checked = true;
      box.dispatchEvent(new Event("change"));
      await wait(50);
      results.forbidden_shows_error = d.getElementById("error").textContent.includes("forbidden");
      results.forbidden_reverts_checkbox = box.checked === false;
    } catch (err) {
      results.error = String(err && err.message ? err.message : err);
    }
    mark();
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=8000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check subscribes_auth_source          "did not subscribe to { source: auth }"
check role_cards_render                "role cards did not render"
check grant_checkbox_reflects_state    "a role's permission checkboxes did not reflect what's granted"
check checkbox_check_sends_grant       "checking a permission box did not send grant-permission"
check mutating_action_refreshes        "a mutating action did not re-subscribe to refresh"
check checkbox_uncheck_sends_revoke    "unchecking a permission box did not send revoke-permission"
check user_row_renders                 "a user row did not render username + name"
check user_role_select_shows_current   "a user's role select did not show their current role"
check role_select_change_sends_assign_role "changing a user's role select did not send assign-role"
check create_role_action               "+ New role did not send create-role"
check create_user_action               "+ New user did not send create-user"
check forbidden_shows_error            "a rejected (Forbidden) action did not show an error"
check forbidden_reverts_checkbox       "a rejected (Forbidden) action did not revert the optimistic checkbox toggle"

[ "$fail" -eq 0 ] && echo "PASS: Roles & Permissions sand is a Protein(auth)+Actions CRUD surface" || exit 1
