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
  .fieldRow--pdf {
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
  .pdfFrame {
    display: block;
    width: 100%;
    height: 100%;
    border: 0;
    background: transparent;
  }

  .image { object-fit: contain; }

  .empty {
    position: absolute;
    inset: 0;
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
