#!/usr/bin/env bash
# Serves the Freedoom sand runtime over HTTP and drives it with Chromium.
# This proves frame.js boots, the bundled wasm/WAD/config become ready, Launch
# renders game pixels, and the local-only sand emits no Protein/Action traffic.
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
PORT=$((40400 + RANDOM % 400))
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
cp "$ROOT/crates/web/src/sand/freedoom/vendor/websockets-doom.js" "$WORK/websockets-doom.js"
cp "$ROOT/crates/web/src/sand/freedoom/vendor/websockets-doom.wasm" "$WORK/websockets-doom.wasm"
cp "$ROOT/crates/web/src/sand/freedoom/vendor/doom1.wad" "$WORK/doom1.wad"
cp "$ROOT/crates/web/src/sand/freedoom/vendor/COPYING.txt" "$WORK/COPYING.txt"
cp "$ROOT/crates/web/src/sand/freedoom/vendor/CREDITS.txt" "$WORK/CREDITS.txt"
sed -n '/^pub(super) const BOOTSTRAP: &str = r#"$/,/^"#;$/p' \
  "$ROOT/crates/web/src/sand/freedoom/script.rs" | sed '1d;$d' > "$WORK/bootstrap.js"
[ -s "$WORK/bootstrap.js" ] || { echo "could not extract Freedoom bootstrap"; exit 1; }

cat > "$WORK/default.cfg" <<'CFG'
mouse_sensitivity 8
show_messages 1
screenblocks 10
detaillevel 0
sfx_volume 8
music_volume 0
use_mouse 1
use_joystick 0
CFG

cat > "$WORK/freedoom.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><script src="/board/frame.js"></script></head><body>
<button id="launch-button" disabled>Launch</button>
<button id="fullscreen-button" disabled>Fullscreen</button>
<button id="reload-button">Reload</button>
<span id="status-dot"></span><span id="status-text"></span>
<pre id="runtime-log"></pre>
<div id="placeholder">placeholder</div>
<div id="canvas-shell"><canvas id="canvas" width="640" height="400"></canvas></div>
<script src="bootstrap.js"></script><script src="websockets-doom.js"></script>
</body></html>
HTML

cat > "$WORK/index.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><title>pending</title></head><body>
<iframe id="sand" src="/freedoom.html" data-package-instance-id="freedoom-test"></iframe>
<script>
  const messages = [];
  addEventListener("message", (event) => {
    if (!event.data || typeof event.data !== "object") return;
    messages.push(event.data);
    if (event.data.type === "lince:ready") {
      event.source.postMessage({ type: "lince:bridge-state", payload: { meta: { cardState: {} } } }, "*");
    }
  });
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  async function poll(fn, timeout = 20000) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      try { const value = fn(); if (value) return value; } catch (_error) {}
      await wait(100);
    }
    return null;
  }
  (async () => {
    const frame = document.getElementById("sand");
    const ready = await poll(() => messages.some((message) => message.type === "lince:ready"));
    const launch = await poll(() => {
      const button = frame.contentDocument?.getElementById("launch-button");
      return button && !button.disabled ? button : null;
    });
    const runtimeReady = frame.contentDocument?.getElementById("status-dot")?.classList.contains("ready") || false;
    launch?.click();
    const launched = await poll(() => {
      const doc = frame.contentDocument;
      return doc?.getElementById("launch-button")?.textContent === "Running"
        && doc?.getElementById("placeholder")?.hidden;
    }, 5000);
    const rendered = await poll(() => {
      const canvas = frame.contentDocument?.getElementById("canvas");
      if (!canvas || canvas.width < 1 || canvas.height < 1) return false;
      const probe = document.createElement("canvas");
      probe.width = 80;
      probe.height = 50;
      const context = probe.getContext("2d", { willReadFrequently: true });
      context.drawImage(canvas, 0, 0, probe.width, probe.height);
      const pixels = context.getImageData(0, 0, probe.width, probe.height).data;
      let lit = 0;
      for (let index = 0; index < pixels.length; index += 4) {
        if (pixels[index] + pixels[index + 1] + pixels[index + 2] > 18) lit += 1;
      }
      return lit > 40;
    }, 30000);
    const forbidden = messages.some((message) =>
      message.type === "lince:protein-subscribe" || message.type === "lince:action");
    document.title = `READY=${!!ready} WASM=${!!runtimeReady} LAUNCHED=${!!launched} RENDERED=${!!rendered} LOCAL=${!forbidden}`;
  })();
</script></body></html>
HTML

(cd "$WORK" && "${SERVER_COMMAND[@]}" >server.log 2>&1) &
SERVER_PID=$!
up=""
for _ in $(seq 1 40); do
  if curl -sf -o /dev/null "$BASE/freedoom.html"; then up=1; break; fi
  sleep 0.1
done
[ -n "$up" ] || { echo "HTTP harness did not start"; cat "$WORK/server.log"; exit 1; }

TITLE="$(timeout 80 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --virtual-time-budget=50000 --dump-dom "$BASE/" 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | tail -1 | sed 's/<[^>]*>//g')"
echo "result: $TITLE"

fail=0
for expected in READY=true WASM=true LAUNCHED=true RENDERED=true LOCAL=true; do
  grep -q "$expected" <<<"$TITLE" || { echo "FAIL: missing $expected"; fail=1; }
done
[ "$fail" -eq 0 ] && echo "PASS: frame lifecycle + bundled Freedoom runtime + nonblank local game canvas" || exit 1
