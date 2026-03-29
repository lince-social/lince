pub(super) const SCRIPT: &str = r##"      const toggle = document.getElementById("mode-toggle");
      const modeLabel = document.getElementById("mode-label");
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

      function escapeHtml(value) {
        return String(value)
          .replaceAll("&", "&amp;")
          .replaceAll("<", "&lt;")
          .replaceAll(">", "&gt;")
          .replaceAll('"', "&quot;")
          .replaceAll("'", "&#39;");
      }

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

      function applyInlineMarkdown(text) {
        return escapeHtml(text)
          .replace(/`([^`]+)`/g, "<code>$1</code>")
          .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
          .replace(/\*([^*]+)\*/g, "<em>$1</em>")
          .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>');
      }

      function renderMarkdown(source) {
        const lines = String(source || "").replace(/\r\n/g, "\n").split("\n");
        const blocks = [];
        let paragraph = [];
        let listItems = [];
        let listTag = "";
        let codeFence = [];
        let inFence = false;

        function flushParagraph() {
          if (!paragraph.length) {
            return;
          }
          blocks.push("<p>" + applyInlineMarkdown(paragraph.join(" ")) + "</p>");
          paragraph = [];
        }

        function flushList() {
          if (!listItems.length) {
            return;
          }
          blocks.push(
            "<" +
              listTag +
              ">" +
              listItems.map((item) => "<li>" + applyInlineMarkdown(item) + "</li>").join("") +
              "</" +
              listTag +
              ">",
          );
          listItems = [];
          listTag = "";
        }

        function flushFence() {
          if (!inFence) {
            return;
          }
          blocks.push("<pre><code>" + escapeHtml(codeFence.join("\n")) + "</code></pre>");
          codeFence = [];
          inFence = false;
        }

        for (const rawLine of lines) {
          const line = rawLine.trimEnd();
          const trimmed = line.trim();

          if (trimmed.startsWith("```")) {
            flushParagraph();
            flushList();
            if (inFence) {
              flushFence();
            } else {
              inFence = true;
              codeFence = [];
            }
            continue;
          }

          if (inFence) {
            codeFence.push(rawLine);
            continue;
          }

          if (!trimmed) {
            flushParagraph();
            flushList();
            continue;
          }

          const heading = trimmed.match(/^(#{1,4})\s+(.+)$/);
          if (heading) {
            flushParagraph();
            flushList();
            const level = heading[1].length;
            blocks.push("<h" + level + ">" + applyInlineMarkdown(heading[2]) + "</h" + level + ">");
            continue;
          }

          if (trimmed === "---" || trimmed === "***") {
            flushParagraph();
            flushList();
            blocks.push("<hr />");
            continue;
          }

          const bullet = trimmed.match(/^[-*]\s+(.+)$/);
          if (bullet) {
            flushParagraph();
            if (listTag && listTag !== "ul") {
              flushList();
            }
            listTag = "ul";
            listItems.push(bullet[1]);
            continue;
          }

          const ordered = trimmed.match(/^\d+\.\s+(.+)$/);
          if (ordered) {
            flushParagraph();
            if (listTag && listTag !== "ol") {
              flushList();
            }
            listTag = "ol";
            listItems.push(ordered[1]);
            continue;
          }

          const quote = trimmed.match(/^>\s+(.+)$/);
          if (quote) {
            flushParagraph();
            flushList();
            blocks.push("<blockquote>" + applyInlineMarkdown(quote[1]) + "</blockquote>");
            continue;
          }

          flushList();
          paragraph.push(trimmed);
        }

        flushParagraph();
        flushList();
        flushFence();
        return blocks.join("");
      }

      function renderPreview() {
        preview.innerHTML = renderMarkdown(rawInput.value);
      }

      function setMode(rendered) {
        toggle.checked = rendered;
        modeLabel.textContent = rendered ? "MD" : "Raw";
        rawInput.hidden = rendered;
        preview.hidden = !rendered;
        writeStorage(modeKey, rendered ? "rendered" : "raw");

        if (rendered) {
          renderPreview();
        } else {
          rawInput.focus();
        }
      }

      rawInput.value = readStorage(noteKey, DEFAULT_TEXT);
      rawInput.addEventListener("input", () => {
        writeStorage(noteKey, rawInput.value);
        if (toggle.checked) {
          renderPreview();
        }
      });

      toggle.addEventListener("change", () => {
        setMode(toggle.checked);
      });

      setMode(readStorage(modeKey, "raw") === "rendered");
    "##;
