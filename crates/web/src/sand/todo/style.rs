pub(super) const INLINE_STYLES: [&str; 1] = [STYLE];

const STYLE: &str = r#"
  :root {
    color-scheme: dark;
    --bg: #0c1117;
    --panel: rgba(17, 22, 29, 0.92);
    --panel-soft: rgba(14, 19, 25, 0.96);
    --line: rgba(255, 255, 255, 0.08);
    --line-strong: rgba(255, 255, 255, 0.16);
    --text: #edf2f7;
    --muted: #91a0b1;
    --accent: #8ab4ff;
    --ok: #8df0b9;
    --warn: #f3c77b;
    --danger: #ff97a8;
    --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
  }

  * {
    box-sizing: border-box;
  }

  html,
  body {
    min-height: 100%;
    margin: 0;
    background: transparent;
  }

  body {
    min-height: 100vh;
    padding: 0;
    background:
      radial-gradient(circle at top right, rgba(138, 180, 255, 0.08), transparent 26%),
      linear-gradient(180deg, rgba(15, 18, 24, 0.99), rgba(11, 13, 17, 0.99));
    color: var(--text);
    font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    overflow: hidden;
  }

  button,
  input,
  textarea {
    font: inherit;
  }

  [hidden] {
    display: none !important;
  }

  .todoWidget {
    height: 100vh;
    max-height: 100vh;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 0;
    overflow: hidden;
  }

  .topLine {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px 8px;
  }

  .topLineSpacer {
    flex: 1 1 auto;
    min-width: 0;
  }

  .topLineActions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-left: auto;
  }

  .contentShell {
    min-height: 0;
    min-width: 0;
    height: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
    gap: 0;
    position: relative;
    overflow: hidden;
  }

  .detailsPanel {
    min-height: 0;
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(360px, 100%);
    padding: 12px 12px 14px;
    border-left: 1px solid var(--line);
    background: #0b1017;
    box-shadow: -28px 0 48px rgba(0, 0, 0, 0.35);
    opacity: 1;
    z-index: 3;
    overflow: auto;
    scrollbar-width: none;
    -ms-overflow-style: none;
  }

  .detailsPanel::-webkit-scrollbar {
    width: 0;
    height: 0;
  }

  .listPanel {
    min-width: 0;
    min-height: 0;
    overflow: auto;
    justify-self: start;
    width: fit-content;
    max-width: 100%;
    overscroll-behavior: contain;
    scrollbar-width: none;
    -ms-overflow-style: none;
    outline: none;
    position: relative;
    z-index: 1;
  }

  .listPanel::-webkit-scrollbar {
    width: 0;
    height: 0;
  }

  .listFrame {
    min-height: 100%;
    min-width: 100%;
    display: inline-block;
    width: max-content;
  }

  .todoList {
    display: grid;
    gap: 2px;
    align-content: start;
    min-width: 100%;
    padding: 8px 0 12px;
  }

  .todoItem {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    align-items: start;
    width: 100%;
    padding: 6px 12px 6px 14px;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }

  .todoItem::before {
    content: "";
    position: absolute;
    left: 0;
    top: 5px;
    bottom: 5px;
    width: 2px;
    border-radius: 999px;
    background: transparent;
    opacity: 0;
    transition: opacity 120ms ease, background 120ms ease;
  }

  .todoItem:hover {
    background: transparent;
  }

  .todoItem:focus-visible {
    outline: none;
  }

  .todoItem[data-active="true"]::before {
    background: var(--accent);
    opacity: 1;
  }

  .todoItem[data-active="true"] .todoItemTitle {
    font-weight: 700;
  }

  .todoItemMain {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .todoItemTitle {
    color: var(--text);
    font-size: 0.84rem;
    font-weight: 500;
    line-height: 1.35;
    word-break: break-word;
  }

  .todoItemBody {
    color: var(--muted);
    font-size: 0.74rem;
    line-height: 1.45;
    word-break: break-word;
  }

  .todoItemMeta {
    align-self: center;
    padding: 4px 9px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: #121923;
    color: var(--muted);
    font-size: 0.7rem;
    line-height: 1;
    white-space: nowrap;
  }

  .emptyState {
    display: grid;
    align-content: start;
    justify-items: start;
    gap: 8px;
    min-height: 100%;
    padding: 24px 12px;
  }

  .stateTitle {
    color: var(--text);
    font-size: 0.96rem;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .stateCopy {
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.5;
    max-width: 44ch;
  }

  .detailStack {
    display: grid;
    gap: 12px;
  }

  .detailCard {
    display: grid;
    gap: 8px;
    padding: 10px 12px 12px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: #111720;
  }

  .detailGrid {
    display: grid;
    gap: 8px;
  }

  .eyebrow {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  .detailTitle {
    color: var(--text);
    font-size: 0.88rem;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .detailCopy {
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.5;
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-height: 32px;
    padding: 0 11px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: #121923;
    color: var(--muted);
    white-space: nowrap;
    font-size: 0.7rem;
  }

  .codeBlock {
    margin: 0;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: #0a0f15;
    color: #d7e3f2;
    font-family: var(--mono);
    font-size: 0.72rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    overflow: auto;
  }

  .statusDot {
    width: 16px;
    height: 16px;
    min-height: 16px;
    padding: 0;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    background: #3c4652;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.14) inset;
    opacity: 0;
    transform: scale(0.85);
    pointer-events: none;
    transition: opacity 140ms ease, transform 140ms ease, box-shadow 140ms ease;
  }

  .todoWidget[data-pointer-active="true"] .statusDot {
    opacity: 1;
    transform: scale(1);
    pointer-events: auto;
  }

  .statusDot[data-tone="live"] {
    background: #1f7c49;
    box-shadow: 0 0 0 1px rgba(141, 240, 185, 0.25) inset, 0 0 12px rgba(141, 240, 185, 0.26);
  }

  .statusDot[data-tone="loading"] {
    background: #9a7420;
    box-shadow: 0 0 0 1px rgba(243, 199, 123, 0.24) inset, 0 0 12px rgba(243, 199, 123, 0.18);
  }

  .statusDot[data-tone="error"] {
    background: #9b2e3c;
    box-shadow: 0 0 0 1px rgba(255, 151, 168, 0.24) inset, 0 0 12px rgba(255, 151, 168, 0.18);
  }

  @media (max-width: 900px) {
    body {
      overflow: auto;
    }

    .contentShell {
      grid-template-columns: minmax(0, 1fr);
    }

    .detailsPanel {
      position: relative;
      width: 100%;
      border-left: 0;
      border-top: 1px solid var(--line);
      box-shadow: none;
      order: 2;
    }
  }
"#;
