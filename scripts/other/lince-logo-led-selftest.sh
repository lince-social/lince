#!/usr/bin/env bash
# Serves the LED sand logic with real frame.js and drives card-state round trips.
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
PORT=$((40800 + RANDOM % 400))
BASE="http://127.0.0.1:$PORT"
PYTHON="$(command -v python3 || true)"
if [ -n "$PYTHON" ]; then
  SERVER_COMMAND=("$PYTHON" -m http.server "$PORT" --bind 127.0.0.1)
else
  command -v nix-shell >/dev/null || { echo "python3 or nix-shell is required"; exit 2; }
  SERVER_COMMAND=(nix-shell -p python3 --run "python3 -m http.server $PORT --bind 127.0.0.1")
fi
cleanup() {
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

mkdir -p "$WORK/board"
cp "$ROOT/crates/web/static/presentation/board/frame.js" "$WORK/board/frame.js"
sed -n '/^pub(super) const SCRIPT: &str = r#"$/,/^"#;$/p' \
  "$ROOT/crates/web/src/sand/lince_logo_led/script.rs" | sed '1d;$d' > "$WORK/logo.js"
[ -s "$WORK/logo.js" ] || { echo "could not extract LED script"; exit 1; }

cat > "$WORK/logo.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><script src="/board/frame.js"></script></head><body>
<main id="stage" data-mode="startup" tabindex="0"><svg><path class="logo-path" d="M0 0 L100 100"></path></svg></main>
<script src="logo.js"></script>
</body></html>
HTML

cat > "$WORK/index.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><title>pending</title></head><body>
<iframe id="sand" src="/logo.html" data-package-instance-id="logo-test"></iframe>
<script>
  const messages = [];
  const frame = document.getElementById("sand");
  addEventListener("message", (event) => {
    if (!event.data || typeof event.data !== "object") return;
    messages.push(event.data);
    if (event.data.type === "lince:ready") {
      event.source.postMessage({
        type: "lince:bridge-state",
        payload: { meta: { cardState: { linceLogoLed: { mode: "pulse" } } } }
      }, "*");
    }
  });
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  async function poll(fn, timeout = 5000) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      try { const value = fn(); if (value) return value; } catch (_error) {}
      await wait(50);
    }
    return null;
  }
  (async () => {
    const hydrated = await poll(() => frame.contentDocument?.getElementById("stage")?.dataset.mode === "pulse");
    frame.contentDocument?.getElementById("stage")?.click();
    const patch = await poll(() => messages.find((message) => message.type === "lince:patch-card-state"));
    frame.contentWindow?.postMessage({
      type: "lince:bridge-state",
      payload: { meta: { cardState: { linceLogoLed: { mode: "phosphor" } } } }
    }, "*");
    const pushed = await poll(() => frame.contentDocument?.getElementById("stage")?.dataset.mode === "phosphor");
    const forbidden = messages.some((message) =>
      message.type === "lince:protein-subscribe" || message.type === "lince:action");
    const patchedScan = patch?.patch?.linceLogoLed?.mode === "scan";
    document.title = `HYDRATED=${!!hydrated} PATCHED=${!!patchedScan} PUSHED=${!!pushed} LOCAL=${!forbidden}`;
  })();
</script></body></html>
HTML

(cd "$WORK" && "${SERVER_COMMAND[@]}" >server.log 2>&1) &
SERVER_PID=$!
up=""
for _ in $(seq 1 40); do
  if curl -sf -o /dev/null "$BASE/logo.html"; then up=1; break; fi
  sleep 0.1
done
[ -n "$up" ] || { echo "HTTP harness did not start"; cat "$WORK/server.log"; exit 1; }

TITLE="$(timeout 40 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --virtual-time-budget=8000 --dump-dom "$BASE/" 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | tail -1 | sed 's/<[^>]*>//g')"
echo "result: $TITLE"

fail=0
for expected in HYDRATED=true PATCHED=true PUSHED=true LOCAL=true; do
  grep -q "$expected" <<<"$TITLE" || { echo "FAIL: missing $expected"; fail=1; }
done
[ "$fail" -eq 0 ] && echo "PASS: LED mode hydrates, cycles, patches, and follows host card state" || exit 1
