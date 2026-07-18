#!/usr/bin/env bash
# Current Ghostty terminal package proof, node-free:
#   - the package declares /board/frame.js and keeps Ghostty notices/assets
#   - the real Ghostty wasm runtime renders bytes received from the host API
#   - the frame is full-bleed and the hover status control owns a closeable panel
#   - ANSI/OSC shell prompts render without visible prompt-control markers
#   - input, resize, restart, and exit all use H.openTerminalSession
#   - no sand-owned WebSocket, Protein subscription, or Action is attempted
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TERMINAL="$ROOT/crates/web/src/sand/terminal"
WORK="$(mktemp -d)"
cleanup() {
  [ -n "${CHROME_PID:-}" ] && kill "$CHROME_PID" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

grep -q 'script src="/board/frame.js"' "$TERMINAL/mod.rs" || {
  echo "FAIL: package does not load /board/frame.js"; exit 1;
}
for notice in LICENSE.txt UPSTREAM.txt; do
  [ -s "$TERMINAL/vendor/$notice" ] || { echo "FAIL: missing vendored notice $notice"; exit 1; }
  grep -q "$notice" "$TERMINAL/mod.rs" || { echo "FAIL: $notice is not bundled"; exit 1; }
done
[ -s "$TERMINAL/vendor/ghostty-vt.wasm" ] || { echo "FAIL: missing Ghostty wasm"; exit 1; }

mkdir -p "$WORK/app" "$WORK/vendor"
cp "$TERMINAL"/app/*.js "$WORK/app/"
cp "$TERMINAL/vendor/ghostty-vt.wasm" "$WORK/vendor/"
cp "$TERMINAL/styles.css" "$WORK/"
{
  printf 'window.__ghosttyWasmBase64="'
  base64 -w 0 "$TERMINAL/vendor/ghostty-vt.wasm"
  printf '";\n'
} > "$WORK/wasm-fixture.js"

cat > "$WORK/terminal-frame.template.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><title>Ghostty Terminal</title>
<link rel="stylesheet" href="styles.css"></head><body>
<main class="ghosttyTerminal">
  <section id="viewport" class="ghosttyViewport" tabindex="0" aria-label="Ghostty terminal">
    <style id="ghostty-theme"></style><div id="buffer" class="ghosttyBuffer"></div>
  </section>
  <button id="connection-button" class="ghosttyConnection" type="button" data-tone="busy"
    aria-controls="info-panel" aria-expanded="false" aria-label="Open terminal controls; connection is starting"></button>
  <aside id="info-panel" class="ghosttyPanel" aria-label="Terminal controls" hidden>
    <div class="ghosttyPanelHeader"><strong>Ghostty VT</strong><button id="close-panel-button" class="ghosttyIconButton" type="button">X</button></div>
    <div class="ghosttyConnectionInfo"><span id="panel-status-dot" class="ghosttyPanelDot" data-tone="busy"></span>
      <div><div id="status-pill" class="ghosttyStatus">Booting</div><div id="session-meta" class="ghosttyMeta">Starting shell</div></div>
    </div>
    <p class="ghosttyLicense">Terminal emulation is powered by libghostty-vt and distributed under the MIT License.</p>
    <nav class="ghosttyLinks"><a href="vendor/UPSTREAM.txt">Upstream</a><a href="vendor/LICENSE.txt">MIT License</a></nav>
    <div class="ghosttyActions"><button id="interrupt-button" class="ghosttyButton ghosttyButton--danger" type="button">Ctrl+C</button><button id="restart-button" class="ghosttyButton" type="button">Restart shell</button></div>
  </aside>
  <div id="measure" class="ghosttyMeasure" aria-hidden="true"><span id="measure-width">MMMMMMMMMM</span><span id="measure-height">M</span></div>
</main>
<script src="wasm-fixture.js"></script>
<script>
  window.__forbidden = [];
  window.requestAnimationFrame = (callback) => window.setTimeout(() => callback(performance.now()), 0);
  window.cancelAnimationFrame = (handle) => window.clearTimeout(handle);
  window.ResizeObserver = class TestResizeObserver {
    constructor(callback) { window.__resizeCallback = callback; }
    observe() {}
    disconnect() {}
  };
  const nativeFetch = window.fetch.bind(window);
  window.fetch = async (resource, options) => {
    const url = String(resource?.url || resource || "");
    if (!url.endsWith("vendor/ghostty-vt.wasm")) return nativeFetch(resource, options);
    const binary = atob(window.__ghosttyWasmBase64);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
    return new Response(bytes, { status: 200, headers: { "content-type": "application/wasm" } });
  };
  window.WebSocket = class ForbiddenWebSocket {
    constructor(url) { window.__forbidden.push("ws:" + url); throw new Error("sand opened a WebSocket"); }
  };
</script>
<script src="/board/frame.js"></script>
<script type="module" src="app/main.js"></script>
</body></html>
HTML

awk -v framefile="$ROOT/crates/web/static/presentation/board/frame.js" '
  index($0, "<script src=\"/board/frame.js\"></script>") {
    print "<script>"
    while ((getline line < framefile) > 0) print line
    close(framefile)
    print "</script>"
    next
  }
  { print }
' "$WORK/terminal-frame.template.html" > "$WORK/terminal-frame.html"
grep -q "openTerminalSession(options" "$WORK/terminal-frame.html" || {
  echo "FAIL: real frame.js terminal API was not inlined"; exit 1;
}

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><title>pending</title></head><body>
<script>
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const sent = [];
  const opens = [];
  const writes = [];
  const resizes = [];
  const closes = [];
  const frame = document.createElement("iframe");
  frame.dataset.packageInstanceId = "terminal-card";
  frame.src = "terminal-frame.html";
  frame.style.cssText = "width:900px;height:620px;border:0";

  const encodeText = (value) => btoa(value);
  const decodeBytes = (value) => Array.from(atob(value), (char) => char.charCodeAt(0));
  window.addEventListener("message", (event) => {
    if (event.source !== frame.contentWindow || !event.data) return;
    const message = event.data;
    sent.push(message);
    if (message.type === "lince:terminal-open") {
      opens.push(message);
      frame.contentWindow.postMessage({
        type: "lince:terminal-opened",
        sessionId: message.sessionId,
        shell: "/bin/sh",
        cwd: "/workspace",
      }, "*");
      setTimeout(() => frame.contentWindow.postMessage({
        type: "lince:terminal-data",
        sessionId: message.sessionId,
        dataBase64: encodeText("\x1b[1;32m[\x1b]0;user@computer: ~\x07user@computer:~]$\x1b[0m __terminal_bridge__\r\n"),
      }, "*"), 40);
    } else if (message.type === "lince:terminal-input") {
      writes.push(decodeBytes(message.dataBase64));
    } else if (message.type === "lince:terminal-resize") {
      resizes.push(message.geometry);
    } else if (message.type === "lince:terminal-close") {
      closes.push(message.sessionId);
    }
  });
  document.body.appendChild(frame);

  (async () => {
    const results = {};
    await wait(900);
    const child = frame.contentWindow;
    const doc = frame.contentDocument;
    const viewport = doc.getElementById("viewport");
    results.ready = sent.some((message) => message.type === "lince:ready");
    results.online = doc.getElementById("status-pill").textContent === "Online";
    results.output = doc.getElementById("buffer").textContent.includes("__terminal_bridge__");
    results.prompt_clean = doc.getElementById("buffer").textContent.includes("[user@computer:~]$")
      && !doc.getElementById("buffer").textContent.includes("\\[");
    results.session_meta = doc.getElementById("session-meta").textContent.includes("/workspace");
    const terminal = doc.querySelector(".ghosttyTerminal");
    const terminalStyle = child.getComputedStyle(terminal);
    const viewportStyle = child.getComputedStyle(viewport);
    const bufferStyle = child.getComputedStyle(doc.getElementById("buffer"));
    results.full_bleed = !doc.querySelector("header, footer, .ghosttyTitle")
      && terminalStyle.padding === "0px"
      && terminalStyle.borderTopWidth === "1px"
      && viewportStyle.borderRadius === "0px"
      && bufferStyle.padding === "0px";

    const connectionButton = doc.getElementById("connection-button");
    connectionButton.click();
    const panel = doc.getElementById("info-panel");
    results.panel = !panel.hidden && connectionButton.hidden
      && panel.textContent.includes("MIT License")
      && panel.textContent.includes("Ctrl+C")
      && panel.textContent.includes("Restart shell");
    doc.getElementById("close-panel-button").click();
    results.panel_close = panel.hidden && !connectionButton.hidden;
    connectionButton.click();

    doc.getElementById("interrupt-button").click();
    viewport.focus();
    viewport.dispatchEvent(new KeyboardEvent("keydown", {
      key: "a", code: "KeyA", bubbles: true, cancelable: true,
    }));
    await wait(100);
    results.input = writes.some((bytes) => bytes.length === 1 && bytes[0] === 3)
      && writes.some((bytes) => bytes.includes(97));

    viewport.style.width = "360px";
    child.__resizeCallback?.();
    await wait(250);
    results.resize = resizes.some((size) => size.cols > 1 && size.rows > 0
      && size.pixelWidth > 0 && size.pixelHeight > 0);

    doc.getElementById("restart-button").click();
    await wait(300);
    results.restart = opens.length === 2 && closes.includes(opens[0].sessionId);
    frame.contentWindow.postMessage({
      type: "lince:terminal-exit",
      sessionId: opens[1].sessionId,
      exitCode: 7,
    }, "*");
    await wait(80);
    results.exit = doc.getElementById("status-pill").textContent === "Exit 7";

    doc.getElementById("restart-button").click();
    await wait(200);
    frame.contentWindow.postMessage({
      type: "lince:terminal-exit",
      sessionId: opens[2].sessionId,
      exitCode: null,
    }, "*");
    await wait(80);
    results.null_exit = doc.getElementById("status-pill").textContent === "Closed";
    results.host_only = child.__forbidden.length === 0
      && !sent.some((message) => message.type === "lince:protein-subscribe" || message.type === "lince:action");
    results.binary_api = writes.length > 0
      && sent.filter((message) => message.type === "lince:terminal-input")
        .every((message) => typeof message.dataBase64 === "string");
    document.title = "RESULT=" + JSON.stringify(results);
    console.log("TRESULT=" + JSON.stringify(results));
    console.log("TDONE");
  })();
</script>
</body></html>
HTML

CHROME_LOG="$WORK/chromium.log"
timeout -k 5 30 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --enable-logging=stderr --v=0 \
  "file://$WORK/harness.html" >"$CHROME_LOG" 2>&1 < /dev/null &
CHROME_PID=$!
for _ in $(seq 1 100); do
  grep -aq "TDONE" "$CHROME_LOG" 2>/dev/null && break
  kill -0 "$CHROME_PID" 2>/dev/null || break
  sleep 0.1
done
kill "$CHROME_PID" 2>/dev/null || true
wait "$CHROME_PID" 2>/dev/null || true
CHROME_PID=""

JSON="$(sed -n 's/.*TRESULT=\(.*\)", source:.*/\1/p' "$CHROME_LOG" | tail -1)"
echo "result: $JSON"
if [ -z "$JSON" ]; then
  echo "FAIL: harness produced no result"
  sed -n '1,80p' "$WORK/chromium.log"
  exit 1
fi

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check ready        "real frame.js did not announce lince:ready"
check online       "host session did not reach Online"
check output       "Ghostty did not render host terminal bytes"
check prompt_clean "Ghostty rendered shell prompt control sequences as visible characters"
check session_meta "host session metadata was not rendered"
check full_bleed   "terminal is not a padding-free, square, border-only surface"
check panel        "hover status control did not open the license/control panel"
check panel_close  "terminal control panel did not close cleanly"
check input        "keyboard/control input did not reach the host session"
check resize       "viewport resize did not reach the host session"
check restart      "restart did not close the old host session and open another"
check exit         "host exit did not update terminal status"
check null_exit    "a host exit without a numeric code did not settle as Closed"
check host_only    "terminal used a direct WebSocket, Protein, or Action"
check binary_api   "terminal writes did not remain byte-oriented at the host API"

[ "$fail" -eq 0 ] && echo "PASS: terminal frame host session + Ghostty bytes + lifecycle" || exit 1
