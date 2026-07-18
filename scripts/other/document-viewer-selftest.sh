#!/usr/bin/env bash
# Current document-viewer package proof, node-free:
#   - real index.html and real /board/frame.js
#   - board card-state push/patch through the flat frame protocol
#   - PDF paging and image rendering
#   - no Protein subscription or Action traffic
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VIEWER="$ROOT/crates/web/src/sand/document_viewer"
FRAME="$ROOT/crates/web/static/presentation/board/frame.js"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$WORK/app" "$WORK/vendor"
cp "$VIEWER/app/"*.js "$WORK/app/"
cp "$VIEWER/styles.css" "$WORK/styles.css"
cp "$VIEWER/vendor/jszip.min.js" "$WORK/vendor/jszip.min.js"
cp "$VIEWER/vendor/epub.min.js" "$WORK/vendor/epub.min.js"

awk -v framefile="$FRAME" '
  index($0, "<script src=\"/board/frame.js\"></script>") {
    print "<script>"
    while ((getline line < framefile) > 0) print line
    close(framefile)
    print "</script>"
    next
  }
  { print }
' "$VIEWER/index.html" > "$WORK/document-viewer.html"
grep -q "window.LinceWidgetHost" "$WORK/document-viewer.html" || {
  echo "FAIL: frame.js was not inlined"; exit 1;
}

for notice in EPUBJS-LICENSE.txt JSZIP-LICENSE.txt; do
  [ -s "$VIEWER/vendor/$notice" ] || { echo "FAIL: missing vendored notice $notice"; exit 1; }
  grep -q "$notice" "$VIEWER/mod.rs" || { echo "FAIL: $notice is not bundled"; exit 1; }
done

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><title>pending</title></head><body>
<script>
  const sent = [];
  const results = {};
  const frame = document.createElement("iframe");
  frame.dataset.packageInstanceId = "viewer-card";

  window.addEventListener("message", (event) => {
    if (event.source !== frame.contentWindow || !event.data) return;
    sent.push(event.data);
    if (event.data.type === "lince:ready") {
      results.ready = true;
      frame.contentWindow.postMessage({
        type: "lince:bridge-state",
        payload: { meta: { cardState: { documentViewer: {
          source: "url",
          path: "https://example.invalid/guide.pdf",
          pdfMode: "page",
          pdfPage: 2,
          loaded: true
        } } } }
      }, "*");
    }
  });

  frame.src = "document-viewer.html";
  document.body.appendChild(frame);

  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  (async () => {
    await wait(500);
    const doc = frame.contentDocument;
    const pdf = doc.getElementById("pdf-frame");
    results.state_restored = doc.getElementById("source-select").value === "url"
      && doc.getElementById("path-input").value.endsWith("guide.pdf")
      && doc.getElementById("pdf-mode-select").value === "page";
    results.pdf_rendered = !pdf.hidden && pdf.getAttribute("src").includes("#page=2&view=FitH");

    frame.contentWindow.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    await wait(100);
    results.pdf_paged = pdf.getAttribute("src").includes("#page=3&view=FitH");

    doc.getElementById("unload-button").click();
    const path = doc.getElementById("path-input");
    path.value = "https://example.invalid/scan.png";
    path.dispatchEvent(new Event("input", { bubbles: true }));
    doc.getElementById("load-button").click();
    await wait(100);
    const image = doc.getElementById("image");
    results.image_rendered = !image.hidden && image.src.endsWith("/scan.png");

    results.state_patched = sent.some((message) =>
      message.type === "lince:patch-card-state"
      && message.patch?.documentViewer?.pdfPage === 3)
      && sent.some((message) =>
        message.type === "lince:patch-card-state"
        && message.patch?.documentViewer?.path?.endsWith("scan.png")
        && message.patch.documentViewer.loaded === true);
    results.no_data_plane = !sent.some((message) =>
      message.type === "lince:protein-subscribe" || message.type === "lince:action");
    results.not_legacy = ![...doc.scripts].some((script) =>
      (script.src || "").includes("widget-frame-bootstrap"));

    document.title = "RESULT=" + JSON.stringify(results);
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=5000 --dump-dom harness.html \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g' || true)"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check ready          "frame.js did not announce lince:ready"
check state_restored "card-local documentViewer state was not restored"
check pdf_rendered   "saved PDF did not render at its saved page"
check pdf_paged      "ArrowRight did not advance the PDF page"
check image_rendered "switching to an image URL did not render the image"
check state_patched  "viewer changes did not patch namespaced card state"
check no_data_plane  "local viewer opened Protein or Action traffic"
check not_legacy     "legacy widget bootstrap was present"

[ "$fail" -eq 0 ] && echo "PASS: document viewer package + frame boot + card state + PDF/image behavior" || exit 1
