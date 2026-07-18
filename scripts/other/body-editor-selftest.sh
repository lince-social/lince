#!/usr/bin/env bash
# K6 — the reusable body editor module (`/board/editor.js`), standalone:
# a plain textarea in headless chromium, no bridge, no stubs. Proves:
#   - "/" at a line start opens the slash palette; it filters by block NAME
#     ("/h3") AND by the underlying characters ("/##"); Enter inserts the
#     markdown in place of the query (canonical markdown, no new format)
#   - the Image block (2026-07-17: no more hand-typed URL) opens the system
#     file picker directly — inserts an "uploading…" placeholder and appends
#     a hidden <input type=file>; the Checkbox block inserts "- [ ] "
#   - "@" opens the record picker fed by getNames; filtering + Enter inserts
#     "@slug "; Escape closes the palette
#   - insertion fires a real `input` event (host dirty-tracking works)
#   - renderMarkdown: headings #x1-7, checkboxes (toggle callback with the
#     ORIGINAL line index), markdown + bare image URLs, @ref chips (navigate
#     callback), plain text preserved
#
# Requires: chromium on PATH (NO node). Usage: scripts/other/body-editor-selftest.sh
set -euo pipefail

CHROMIUM="${CHROMIUM:-$(command -v chromium || command -v chromium-browser || true)}"
[ -n "$CHROMIUM" ] || { echo "chromium not found on PATH"; exit 2; }

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp "$ROOT/crates/web/static/presentation/board/editor.js" "$WORK/editor.js"

cat > "$WORK/harness.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"></head><body>
<textarea id="body" style="width:400px;height:120px"></textarea>
<div id="out"></div>
<script src="editor.js"></script>
<script>
  const results = {};
  const mark = () => { document.title = "RESULT=" + JSON.stringify(results); };
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const area = document.getElementById("body");
  const palette = () => document.querySelector(".lince-editor-palette");
  const items = () => [...palette().querySelectorAll(".lince-editor-item")]
    .map((el) => el.firstChild.textContent);
  const type = (value, caret) => {
    area.value = value;
    area.selectionStart = area.selectionEnd = caret ?? value.length;
    area.dispatchEvent(new Event("input", { bubbles: true }));
  };
  const key = (k) => area.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));

  (async () => {
    try {
    const names = [
      { uid: "r_two", slug: "task.two", head: "Task two" },
      { uid: "p_ana", slug: "ana", head: "Ana" },
    ];
    window.LinceBodyEditor.attach(area, { getNames: () => names });
    let inputEvents = 0;
    area.addEventListener("input", () => { inputEvents++; });

    // slash opens with every block; filters by name and by raw characters
    type("/");
    await wait(20);
    results.slash_opens = palette().style.display === "block" && items().length === 9;
    type("/h3");
    await wait(20);
    results.slash_name_filter = items().length === 1 && items()[0] === "Heading 3";
    type("/##");
    await wait(20);
    results.slash_char_filter = items()[0] === "Heading 2";

    // Enter inserts the markdown in place of the query + fires input
    type("/h3");
    await wait(20);
    inputEvents = 0;
    key("Enter");
    results.slash_insert = area.value === "### " && area.selectionStart === 4;
    results.insert_fires_input = inputEvents > 0;

    // image block: opens a file picker instead of a hand-typed URL — an
    // "uploading…" placeholder holds the spot, a hidden file input appears
    type("intro\n/img", undefined);
    await wait(20);
    key("Enter");
    await wait(20);
    results.image_insert = area.value === "intro\n![uploading…]()"
      && !!document.querySelector('input[type="file"]');
    // cancelling the (headless, never-shown) picker leaves no stray markup:
    // simulate it and confirm the placeholder clears back out
    document.querySelector('input[type="file"]').dispatchEvent(new Event("cancel"));
    await wait(20);
    results.image_cancel_clears = area.value === "intro\n";
    type("/check");
    await wait(20);
    key("Enter");
    results.checkbox_insert = area.value === "- [ ] ";

    // slash only at line START; "@" opens the mention picker anywhere
    type("not /h3 here");
    await wait(20);
    results.slash_only_at_start = !palette() || palette().style.display === "none";
    type("ping @ta");
    await wait(20);
    results.mention_filters = palette().style.display === "block"
      && items().length === 1 && items()[0] === "Task two";
    key("ArrowDown"); // wraps on a 1-item list; must not throw
    key("Enter");
    results.mention_insert = area.value === "ping @task.two ";

    // Escape closes
    type("/");
    await wait(20);
    key("Escape");
    results.esc_closes = palette().style.display === "none";

    // renderer: same blocks either way
    const out = document.getElementById("out");
    let toggled = null;
    let navigated = null;
    out.appendChild(window.LinceBodyEditor.renderMarkdown(
      "# Big\n####### Tiny\n- [x] done thing\nplain line\n![](https://x.test/a.png)\nsee https://x.test/b.jpg and @task.two",
      {
        resolveRef: (token) => token === "task.two" ? { uid: "r_two", label: token } : null,
        onToggle: (at) => { toggled = at; },
        onNavigate: (uid) => { navigated = uid; },
      }));
    results.render_headings = !!out.querySelector(".md-h1") && !!out.querySelector(".md-h7");
    const box = out.querySelector('input[data-md-line="2"]');
    results.render_checkbox = !!box && box.checked === true;
    box.click();
    results.toggle_line_index = toggled === 2;
    results.render_images = [...out.querySelectorAll("img")].map((i) => i.src).join(",")
      === "https://x.test/a.png,https://x.test/b.jpg";
    const chip = out.querySelector('[data-ref="r_two"]');
    results.render_ref = !!chip && chip.textContent === "@task.two";
    chip.click();
    results.ref_navigates = navigated === "r_two";
    results.render_plain = out.textContent.includes("plain line");
    } catch (err) {
      results.error = String(err && err.message ? err.message : err);
    }
    mark();
  })();
</script>
</body></html>
HTML

TITLE="$(cd "$WORK" && timeout 60 "$CHROMIUM" --headless --disable-gpu --no-sandbox \
  --allow-file-access-from-files --virtual-time-budget=5000 --dump-dom harness.html 2>/dev/null \
  | grep -oE '<title>[^<]*</title>' | sed 's/<[^>]*>//g')"

echo "result: $TITLE"
JSON="${TITLE#RESULT=}"
[ "$JSON" != "$TITLE" ] || { echo "FAIL: harness produced no result"; exit 1; }

fail=0
check() { grep -q "\"$1\":true" <<<"$JSON" || { echo "FAIL: $2"; fail=1; }; }
check slash_opens         "/ did not open the palette with all 9 blocks"
check slash_name_filter   "/h3 did not filter to Heading 3"
check slash_char_filter   "/## did not match by the underlying characters"
check slash_insert        "Enter did not insert '### ' in place of the query"
check insert_fires_input  "insertion did not fire a real input event"
check image_insert        "Image block did not open a file picker with an uploading placeholder"
check image_cancel_clears "cancelling the file picker did not clear the uploading placeholder"
check checkbox_insert     "Checkbox block did not insert '- [ ] '"
check slash_only_at_start "a mid-line / wrongly opened the palette"
check mention_filters     "@ta did not filter the record picker"
check mention_insert      "Enter did not insert @task.two"
check esc_closes          "Escape did not close the palette"
check render_headings     "renderer missed heading levels 1/7"
check render_checkbox     "renderer did not render the checked checkbox"
check toggle_line_index   "checkbox toggle did not report the ORIGINAL line index"
check render_images       "markdown + bare image URLs did not both render"
check render_ref          "@task.two did not render as a reference chip"
check ref_navigates       "the reference chip did not navigate"
check render_plain        "plain text lines were not preserved"

[ "$fail" -eq 0 ] && echo "PASS: K6 body editor (slash blocks by name/chars, @-mentions, canonical markdown, shared block renderer)" || exit 1
