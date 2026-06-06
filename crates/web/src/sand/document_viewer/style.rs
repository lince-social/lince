pub(super) const INLINE_STYLES: &[&str] = &[r#"
  :root {
    color-scheme: dark;
    --bg: #0f1318;
    --panel: rgba(23, 27, 33, 0.96);
    --panel-soft: #10151b;
    --line: rgba(255, 255, 255, 0.10);
    --text: #edf2f7;
    --muted: #95a0ad;
    --accent: #1ec97f;
    --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
  }

  * { box-sizing: border-box; }

  html, body {
    width: 100%;
    min-height: 100%;
    margin: 0;
    background: var(--bg);
  }

  body {
    min-height: 100vh;
    color: var(--text);
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  }

  button,
  input,
  select {
    font: inherit;
  }

  .app {
    position: relative;
    min-height: 100vh;
    background: var(--bg);
    overflow: hidden;
  }

  .configToggle {
    position: absolute;
    top: 12px;
    right: 12px;
    z-index: 3;
    width: 18px;
    height: 18px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: transparent;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.45), 0 0 12px rgba(30, 201, 127, 0.45);
    cursor: pointer;
    opacity: 1;
    transition: opacity 140ms ease, transform 140ms ease;
  }

  .app.hasDocument .configToggle {
    opacity: 0;
  }

  .app.hasDocument .configToggle:hover,
  .app.hasDocument .configToggle:focus-visible {
    opacity: 1;
  }

  .configToggle:hover,
  .configToggle:focus-visible {
    transform: scale(1.08);
  }

  .configPanel {
    position: absolute;
    top: 0;
    right: 0;
    z-index: 2;
    width: min(360px, 100%);
    height: 100%;
    display: grid;
    align-content: start;
    gap: 8px;
    padding: 12px;
    border-left: 1px solid var(--line);
    background: var(--panel);
    backdrop-filter: blur(10px);
  }

  .controls {
    display: grid;
    gap: 8px;
  }

  .fieldRow {
    display: grid;
    gap: 8px;
  }

  .fieldRow--source,
  .fieldRow--path,
  .fieldRow--mode,
  .fieldRow--epub {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .actionRow {
    display: flex;
    gap: 8px;
  }

  .field,
  .button {
    min-height: 36px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel-soft);
    color: var(--text);
  }

  .field {
    width: 100%;
    padding: 0 10px;
    outline: none;
  }

  .field--select { cursor: pointer; }

  .button {
    padding: 0 12px;
    cursor: pointer;
  }

  .button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .licenseRow {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    align-items: center;
    color: var(--muted);
    font-size: 0.72rem;
    line-height: 1.35;
  }

  .licenseRow a {
    color: var(--accent);
    text-decoration: none;
  }

  .licenseRow a:hover {
    text-decoration: underline;
  }

  .debug {
    margin: 0;
    min-height: 40px;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: #12161d;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.68rem;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .documentArea {
    min-height: 100vh;
    height: 100vh;
  }

  .frame {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    border: 0;
    border-radius: 0;
    background: #0f1318;
  }

  .image,
  .pdfFrame,
  .epubViewer {
    display: block;
    width: 100%;
    height: 100%;
    border: 0;
    background: transparent;
  }

  .image { object-fit: contain; }

  .frame.isImageScroll {
    overflow: auto;
  }

  .frame.isImageScroll .image {
    width: 100%;
    height: auto;
    min-height: 100%;
    object-fit: contain;
  }

  .epubViewer {
    overflow: hidden;
    background: #f8f5ef;
    color: #171717;
  }

  .epubViewer.isScroll {
    overflow: auto;
  }

  .epubViewer iframe {
    background: #f8f5ef;
  }

  .navHit {
    position: absolute;
    inset: 0;
    z-index: 1;
    cursor: pointer;
    background: transparent;
    pointer-events: none;
  }

  .navZone {
    position: absolute;
    top: 0;
    bottom: 0;
    z-index: 1;
    width: 50%;
    padding: 0;
    border: 0;
    border-radius: 0;
    appearance: none;
    color: transparent;
    background: transparent;
    box-shadow: none;
    outline: none;
    pointer-events: auto;
  }

  .navZone:focus,
  .navZone:focus-visible {
    outline: none;
  }

  .navZone--prev { left: 0; }
  .navZone--next { right: 0; }

  .navHit.isEdgeOnly .navZone {
    width: 18%;
  }

  .empty {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: grid;
    align-content: center;
    justify-items: center;
    gap: 8px;
    padding: 18px;
    text-align: center;
  }

  .emptyEyebrow {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.66rem;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .emptyTitle {
    font-size: 0.96rem;
    font-weight: 700;
  }

  .emptyCopy {
    max-width: 40ch;
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.45;
  }

  [hidden] { display: none !important; }
"#];
