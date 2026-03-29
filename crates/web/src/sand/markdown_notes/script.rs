pub(super) fn script() -> String {
    let mut script = String::from(crate::sand::shared_markdown::JS_HELPERS);
    script.push_str(
        r##"      const toggle = document.getElementById("mode-toggle");
      const rawInput = document.getElementById("raw-input");
      const preview = document.getElementById("preview");
      const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";
      const noteKey = "markdown-notes/content/" + instanceId;
      const modeKey = "markdown-notes/mode/" + instanceId;

      const DEFAULT_TEXT = [
        "# Notas",
        "",
        "Escreva em **Markdown**.",
        "",
        "- `Raw` para editar",
        "- `MD` para renderizar",
      ].join("\n");

      function readStorage(key, fallback) {
        try {
          const value = window.localStorage.getItem(key);
          return value == null ? fallback : value;
        } catch (error) {
          return fallback;
        }
      }

      function writeStorage(key, value) {
        try {
          window.localStorage.setItem(key, value);
        } catch (error) {
          return;
        }
      }

      function syncRenderedSignal() {
        toggle.dispatchEvent(new Event("input", { bubbles: true }));
        toggle.dispatchEvent(new Event("change", { bubbles: true }));
      }

      function renderPreview() {
        preview.innerHTML = renderMarkdown(rawInput.value);
      }

      rawInput.value = readStorage(noteKey, DEFAULT_TEXT);
      rawInput.addEventListener("input", () => {
        writeStorage(noteKey, rawInput.value);
        if (toggle.checked) {
          renderPreview();
        }
      });

      toggle.addEventListener("change", () => {
        writeStorage(modeKey, toggle.checked ? "rendered" : "raw");
        if (toggle.checked) {
          renderPreview();
        } else {
          rawInput.focus();
        }
      });

      toggle.checked = readStorage(modeKey, "raw") === "rendered";
      syncRenderedSignal();
      if (toggle.checked) {
        renderPreview();
      } else {
        rawInput.focus();
      }
    "##,
    );
    script
}
