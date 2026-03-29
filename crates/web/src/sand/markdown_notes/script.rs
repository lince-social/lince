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

      const state = {
        rawText: "",
        rendered: false,
      };

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

      function syncSignalsFromDom() {
        rawInput.dispatchEvent(new Event("input", { bubbles: true }));
        rawInput.dispatchEvent(new Event("change", { bubbles: true }));
        toggle.dispatchEvent(new Event("input", { bubbles: true }));
        toggle.dispatchEvent(new Event("change", { bubbles: true }));
      }

      preview.addEventListener("change", (event) => {
        const checkbox = event.target.closest("input[data-markdown-task-line]");
        if (!checkbox) {
          return;
        }

        const nextRawText = toggleMarkdownTaskLine(
          rawInput.value,
          checkbox.dataset.markdownTaskLine,
          checkbox.checked,
        );

        if (nextRawText === rawInput.value) {
          return;
        }

        rawInput.value = nextRawText;
        window.MarkdownNotes?.sync(nextRawText, toggle.checked);
        syncSignalsFromDom();
      });

      window.MarkdownNotes = {
        sync(rawText, rendered) {
          const nextRawText = String(rawText ?? "");
          const nextRendered = rendered === true;
          if (
            state.rawText === nextRawText &&
            state.rendered === nextRendered
          ) {
            return;
          }
          state.rawText = nextRawText;
          state.rendered = nextRendered;
          preview.innerHTML = renderMarkdown(nextRawText);
          writeStorage(noteKey, nextRawText);
          writeStorage(modeKey, nextRendered ? "rendered" : "raw");
          if (!nextRendered) {
            rawInput.focus();
            rawInput.setSelectionRange(rawInput.value.length, rawInput.value.length);
          }
        },
      };

      rawInput.value = readStorage(noteKey, DEFAULT_TEXT);
      toggle.checked = readStorage(modeKey, "raw") === "rendered";
      state.rawText = rawInput.value;
      state.rendered = toggle.checked;
      preview.innerHTML = renderMarkdown(rawInput.value);
      syncSignalsFromDom();

      if (!toggle.checked) {
        rawInput.focus();
        rawInput.setSelectionRange(rawInput.value.length, rawInput.value.length);
      }
    "##,
    );
    script
}
