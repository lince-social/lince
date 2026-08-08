#!/usr/bin/env bash
# End-to-end verification of the Archive sand's static workspace export with
# the REAL pieces, node-free (same harness pattern as
# relations-group-e2e-selftest.sh):
#   - the real Archive sand `archive.html` (its real button + filename input)
#   - the real `frame.js` sand host running INSIDE real iframes
#   - the real unified `widget-bridge.js` + shared `transport.js`
#   - the real capture/compose module `archive.js` wired exactly as `main.js`
#     wires `archiveWorkspace`
# Only the transport WebSocket is stubbed.
#
# It proves: clicking the Archive button routes `lince:archive-workspace`
# through frame.js + the bridge (source-guarded) into buildWorkspaceArchive;
# the produced single-file HTML carries the rendered DATA (text marker, typed
# input value, checked checkbox, rasterized canvas) but NO password value, NO
# <script>, NO on* handler and NO external URL (evil CSS url() neutralized);
# cards land at bounding-rect offsets with z-order and shrink-to-fit stage
# size; the requesting Archive card and system/pinned shell cards are
# excluded; text cards render escaped; an impostor postMessage from outside
# the card's iframe is rejected; the capture sends NOTHING over the
# transport; and the produced file re-opens in chromium with its sandboxed
# srcdoc iframes and zero external references.
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/archive-sand-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BOARD="$ROOT/crates/web/static/presentation/board"
SAND="$ROOT/crates/web/src/sand"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Bundle transport + unified bridge + archive module as one classic script.
sed 's/^export function/function/; s/^export async function/async function/; /^import /{:a;/;$/!{N;ba};d}' \
  "$BOARD/transport.js" > "$WORK/bundle.js"
sed 's/^export function/function/; s/^export async function/async function/; /^import /{:a;/;$/!{N;ba};d}' \
  "$BOARD/widget-bridge.js" >> "$WORK/bundle.js"
sed 's/^export function/function/; s/^export async function/async function/; /^import /{:a;/;$/!{N;ba};d}' \
  "$BOARD/archive.js" >> "$WORK/bundle.js"
grep -q "function buildWorkspaceArchive" "$WORK/bundle.js" || { echo "archive.js bundle failed"; exit 1; }

# Inline a `<script src="..."></script>` tag with a local file's contents
# (file:// cannot resolve the board's absolute URLs). awk does the inlining.
inline_script() {
  local src="$1" tag="$2" file="$3" out="$4"
  awk -v tag="$tag" -v inlinefile="$file" '
    index($0, tag) {
      print "<script>"
      while ((getline line < inlinefile) > 0) print line
      close(inlinefile)
      print "</script>"
      next
    }
    { print }
  ' "$src" > "$out"
}

# Content sand A: rendered data + a canvas + form state + hostile bits that
# must NOT survive the capture (script, onclick, external CSS url()).
cat > "$WORK/sand-a-src.html" <<'HTML'
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Sand A</title>
  <script src="/board/frame.js"></script>
  <style>
    body { font: 12px sans-serif; }
    .evil { background-image: url("https://evil.example/x.png"); }
  </style>
</head>
<body>
  <p id="marker">HELLO-ARCHIVE-DATA</p>
  <div class="evil" onclick="alert(1)">clickable</div>
  <input id="plain" type="text">
  <input id="pw" type="password">
  <input id="chk" type="checkbox">
  <canvas id="cv" width="40" height="30"></canvas>
  <script>
    const context = document.getElementById("cv").getContext("2d");
    context.fillStyle = "#ff0000";
    context.fillRect(0, 0, 40, 30);
  </script>
</body>
</html>
HTML
inline_script "$WORK/sand-a-src.html" '<script src="/board/frame.js"></script>' \
  "$BOARD/frame.js" "$WORK/sand-a.html"
grep -q "LinceWidgetHost" "$WORK/sand-a.html" || { echo "frame.js inline failed for sand A"; exit 1; }

# The REAL Archive sand.
inline_script "$SAND/archive/archive.html" '<script src="/board/frame.js"></script>' \
  "$BOARD/frame.js" "$WORK/archive-frame.html"
grep -q "LinceWidgetHost" "$WORK/archive-frame.html" || { echo "frame.js inline failed for Archive"; exit 1; }

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8">
<style>iframe{width:640px;height:480px;border:0}</style>
</head><body>
<div id="status"></div>
<textarea id="out"></textarea>
<script src="bundle.js"></script>
<script>
  window.__sent = [];
  window.__wsCount = 0;
  class FakeWS {
    constructor(url) { this.url = url; this.readyState = 0; window.__ws = this;
      window.__wsCount += 1; this._open = [];
      setTimeout(() => { this.readyState = 1; this._open.forEach((f) => f()); }, 0); }
    addEventListener(t, f) { if (t === "open") this._open.push(f);
      else if (t === "message") this._msg = f; else if (t === "close") this._close = f;
      else if (t === "error") this._err = f; }
    send(raw) { window.__sent.push(JSON.parse(raw)); }
    close() { this.readyState = 3; }
  }
  FakeWS.CONNECTING=0; FakeWS.OPEN=1; FakeWS.CLOSING=2; FakeWS.CLOSED=3;
  window.WebSocket = FakeWS;

  // Store-snapshot card model: one content sand, one text card, the Archive
  // sand itself, and a system/pinned shell pin that must never be exported.
  const cards = [
    { id: "card-a", kind: "package", title: "Conteudo A", text: "",
      x: 49000, y: 49100, width: 400, height: 300, zIndex: 2,
      pinned: false, system: false },
    { id: "card-text", kind: "text", title: "Nota <b>rica</b>",
      text: "corpo da nota", x: 49500, y: 49000, width: 200, height: 150,
      zIndex: 1, pinned: false, system: false },
    { id: "card-archive", kind: "package", title: "Archive", text: "",
      x: 50000, y: 50000, width: 300, height: 300, zIndex: 3,
      pinned: false, system: false },
    { id: "card-shell", kind: "package", title: "SHELL-PIN-MARKER", text: "",
      x: 0, y: 0, width: 100, height: 100, zIndex: 50,
      pinned: true, system: true },
  ];

  function makeIframe(id, file) {
    const f = document.createElement("iframe");
    f.className = "package-widget__frame";
    f.dataset.packageInstanceId = id; // set BEFORE load so frame.js reads it
    f.src = file;
    document.body.appendChild(f);
    return f;
  }
  const sandA = makeIframe("card-a", "sand-a.html");
  const archiveFrame = makeIframe("card-archive", "archive-frame.html");
  const frames = [sandA, archiveFrame];
  const frameById = { "card-a": sandA, "card-archive": archiveFrame };

  window.__archiveCalls = 0;
  window.__archive = null;
  window.__archiveError = "";

  const bridge = createWidgetBridge({
    statusNode: document.getElementById("status"),
    getFrames: () => frames,
    initialState: {},
    getCardMeta: () => ({}),
    getCardAbiListen: () => [],
    getCardGroupStack: () => [],
    setCardState: () => {}, patchCardState: () => {}, setCardStreamsEnabled: () => {},
    handleShellAction: () => {}, invalidateServerAuth: () => {},
    // Wired exactly like main.js's runWorkspaceArchive, minus the download.
    async archiveWorkspace(instanceId, options) {
      window.__archiveCalls += 1;
      try {
        window.__archive = await buildWorkspaceArchive({
          workspaceName: "Meu Portfólio",
          cards,
          excludeCardIds: [instanceId],
          frameForCard: (id) => frameById[id] || null,
          filename: options && options.filename,
        });
      } catch (error) {
        window.__archiveError = String((error && error.message) || error);
      }
    },
    onError: () => {},
  });

  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  async function until(fn, timeoutMs) {
    const deadline = Date.now() + (timeoutMs || 4000);
    for (;;) {
      let value = null;
      try { value = fn(); } catch (error) { value = null; }
      if (value) return value;
      if (Date.now() > deadline) return null;
      await wait(50);
    }
  }

  (async () => {
    const results = {};

    const sandReady = await until(() =>
      sandA.contentDocument && sandA.contentDocument.getElementById("cv") &&
      sandA.contentWindow.LinceWidgetHost ? true : null);
    const archiveReady = await until(() =>
      archiveFrame.contentDocument &&
      archiveFrame.contentDocument.getElementById("archive") &&
      archiveFrame.contentWindow.LinceWidgetHost ? true : null);
    results.frames_ready = Boolean(sandReady && archiveReady);

    // Runtime state that only exists in DOM properties, never in markup.
    const aDoc = sandA.contentDocument;
    aDoc.getElementById("plain").value = "TYPED-PLAIN-VALUE-42";
    aDoc.getElementById("pw").value = "SECRET-TOKEN-XYZ";
    aDoc.getElementById("chk").checked = true;

    // The REAL flow: type a filename and click the REAL Archive button.
    const arcDoc = archiveFrame.contentDocument;
    arcDoc.getElementById("filename").value = "Meu Portfólio!";
    arcDoc.getElementById("archive").click();
    await until(() => window.__archive || window.__archiveError || null);

    // Impostor: same message type from the harness window (NOT the card's
    // iframe) must be dropped by the bridge's isCurrentFrameSource guard.
    window.postMessage(
      { type: "lince:archive-workspace", instanceId: "card-a", options: {} }, "*");
    await wait(250);

    const A = window.__archive;
    results.error = window.__archiveError || "";
    results.produced = Boolean(A && A.filename === "meu-portfolio.html"
      && A.included === 2 && A.skipped.length === 0);
    const html = (A && A.html) || "";

    const outer = new DOMParser().parseFromString(html, "text/html");
    const cardFrames = outer.querySelectorAll("iframe.card");
    const subHtml = cardFrames.length === 1 ? cardFrames[0].getAttribute("srcdoc") : "";
    const sub = new DOMParser().parseFromString(subHtml || "", "text/html");

    results.iframes = cardFrames.length === 1
      && html.includes('<iframe class="card" sandbox')
      && html.includes('srcdoc="');
    results.csp = html.includes('http-equiv="Content-Security-Policy"')
      && html.includes("default-src 'none'");
    results.data_present = html.includes("HELLO-ARCHIVE-DATA")
      && (sub.getElementById("plain")?.getAttribute("value") === "TYPED-PLAIN-VALUE-42");
    results.secret_absent = !html.includes("SECRET-TOKEN-XYZ")
      && !(sub.getElementById("pw")?.hasAttribute("value"));
    results.checkbox_checked = Boolean(sub.getElementById("chk")?.hasAttribute("checked"));
    results.canvas_rasterized = Boolean(
      sub.querySelector('img[src^="data:image/png"]'))
      && sub.querySelectorAll("canvas").length === 0;
    results.no_scripts = !html.includes("<script")
      && sub.querySelectorAll("script").length === 0;
    results.no_onclick = !subHtml.includes("onclick");
    results.css_neutralized = !html.includes("evil.example")
      && (subHtml.includes('url("data:,")') || html.includes("url(&quot;data:,&quot;)"));
    results.offsets =
      html.includes("left:24px;top:124px;width:400px;height:300px;")
      && html.includes("left:524px;top:24px;width:200px;height:150px;");
    results.zorder = html.includes("z-index:2;") && html.includes("z-index:1;");
    results.archive_excluded = !html.includes("Archive this workspace");
    results.shell_excluded = !html.includes("SHELL-PIN-MARKER");
    results.text_card = html.includes("Nota &lt;b&gt;rica&lt;/b&gt;")
      && html.includes("corpo da nota");
    results.stage_size = html.includes("width:748px;height:448px");
    results.guard = window.__archiveCalls === 1;
    results.no_transport = !window.__sent.some((m) =>
      JSON.stringify(m).toLowerCase().includes("archive"));
    results.one_socket = window.__wsCount <= 1;

    // Hand the produced file to the shell for the second chromium pass.
    document.getElementById("out").textContent =
      "B64S" + btoa(unescape(encodeURIComponent(html))) + "B64E";
    document.title = "RESULT=" + JSON.stringify(results);
  })();
</script>
</body></html>
HTML

DUMP="$WORK/dump.html"
(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=9000 --dump-dom harness.html 2>/dev/null) > "$DUMP"

TITLE="$(grep -oE '<title>[^<]*</title>' "$DUMP" | sed 's/<[^>]*>//g')"
echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result (iframe/module load?)"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check frames_ready      "the sand iframes (frame.js hosts) never became ready"
check produced          "no archive produced, or wrong filename/included/skipped (see error field above)"
check iframes           "output does not embed exactly one sandboxed srcdoc card iframe"
check csp               "the CSP meta (default-src 'none') is missing from the output head"
check data_present      "rendered data (text marker / typed input value) did not reach the output"
check secret_absent     "the password value leaked into the output"
check checkbox_checked  "runtime checkbox state was not reflected as a checked attribute"
check canvas_rasterized "the canvas was not rasterized to a data: PNG img"
check no_scripts        "a <script> survived the capture"
check no_onclick        "an on* handler survived the capture"
check css_neutralized   "the external CSS url() was not neutralized to data:,"
check offsets           "cards are not at their bounding-rect offsets (+24px padding)"
check zorder            "card z-index values were not carried into the output"
check archive_excluded  "the Archive sand exported ITSELF"
check shell_excluded    "a system/pinned shell card leaked into the output"
check text_card         "the text card is missing or its title was not HTML-escaped"
check stage_size        "the stage is not sized to the bounding rect + 2*24px"
check guard             "an impostor postMessage outside the card iframe triggered an archive"
check no_transport      "the archive flow sent something over the transport socket"
check one_socket        "more than one transport socket was opened"

# ---- Phase 2: the produced file itself ------------------------------------
grep -oE 'B64S[A-Za-z0-9+/=]+B64E' "$DUMP" | head -1 | sed 's/^B64S//; s/B64E$//' \
  | base64 -d > "$WORK/out.html" 2>/dev/null || true
[ -s "$WORK/out.html" ] || { echo "FAIL: could not extract the produced archive file"; exit 1; }

if grep -q "<script" "$WORK/out.html"; then
  echo "FAIL: produced file contains a <script>"; fail=1; fi
if grep -qE 'https?://' "$WORK/out.html"; then
  echo "FAIL: produced file references an external http(s) URL"; fail=1; fi
if grep -qE '(src|href)=("|&quot;)/' "$WORK/out.html"; then
  echo "FAIL: produced file references a root-relative URL"; fail=1; fi

REDUMP="$(timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=3000 --dump-dom "$WORK/out.html" 2>/dev/null)"
grep -q 'class="card" sandbox' <<<"$REDUMP" || { echo "FAIL: reopened file lost its sandboxed card iframes"; fail=1; }
grep -q "HELLO-ARCHIVE-DATA" <<<"$REDUMP" || { echo "FAIL: reopened file lost the captured sand data"; fail=1; }
grep -q "Nota" <<<"$REDUMP" || { echo "FAIL: reopened file lost the text card"; fail=1; }

[ "$fail" -eq 0 ] && echo "PASS: archive sand static export — real button through the real bridge (source-guarded, transport-silent), data captured (inputs/checkbox/canvas) with secrets/scripts/on*/external URLs stripped, bounding-rect layout with z-order and shrink-to-fit stage, self/shell exclusion, and the produced file re-opens clean with zero external references" || exit 1
