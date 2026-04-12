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

  .tableWidget {
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

  .blobLayer {
    position: absolute;
    inset: 0;
    z-index: 2;
    overflow: hidden;
    pointer-events: none;
  }

  .blobLayer canvas {
    display: block;
    width: 100%;
    height: 100%;
    filter:
      contrast(1.25)
      saturate(1.35)
      drop-shadow(0 0 10px rgba(81, 243, 210, 0.24));
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
  }

  .tablePanel {
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

  .tablePanel::-webkit-scrollbar {
    width: 0;
    height: 0;
  }

  .tableFrame {
    min-height: 0;
    min-width: 0;
    display: inline-block;
    width: max-content;
  }

  .tablePanel[data-mode="helix"] tr[data-row-focused="true"] td[data-column-key="head"] {
    position: relative;
    background: transparent !important;
    box-shadow: none !important;
  }

  .tablePanel[data-mode="helix"] tr[data-row-focused="true"] td[data-column-key="head"]::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--accent);
    border-radius: 999px;
    opacity: 1;
  }

  .tablePanel[data-mode="helix"][data-blob="true"] tr[data-row-focused="true"] td[data-column-key="head"]::before {
    opacity: 0;
  }

  .tablePanel[data-mode="helix"][data-blob="true"] thead th[data-column-key="head"],
  .tablePanel[data-mode="helix"][data-blob="true"] tbody td[data-column-key="head"] {
    padding-left: 2rem;
  }

  .tablePanel tr[data-row-dispersing="true"] {
    opacity: 0;
  }

  .tablePanel thead th[data-column-key]:not([data-column-key="head"]),
  .tablePanel tbody td[data-column-key]:not([data-column-key="head"]) {
    display: none;
  }

  .detailStack {
    display: grid;
    gap: 12px;
  }

  .detailStack--settings {
    margin-bottom: 12px;
  }

  .detailCard {
    display: grid;
    gap: 8px;
    padding: 10px 12px 12px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: #111720;
  }

  .detailCard--setting {
    gap: 10px;
    align-items: start;
  }

  .settingRow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .settingBlock {
    display: grid;
    gap: 7px;
    width: 100%;
  }

  .settingLabel {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .toggleRow {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
    font-size: 0.78rem;
    font-weight: 600;
  }

  .toggleRow input {
    accent-color: var(--accent);
  }

  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }

  .colorTools {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .colorInput {
    width: 42px;
    height: 32px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: #121923;
  }

  .palette {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    min-height: 24px;
  }

  .paletteSwatch {
    width: 22px;
    height: 22px;
    border: 1px solid rgba(255, 255, 255, 0.22);
    border-radius: 7px;
    box-shadow: 0 0 12px rgba(81, 243, 210, 0.14);
    cursor: pointer;
  }

  .licenseLink {
    color: var(--muted);
    font-size: 0.7rem;
    text-decoration: none;
  }

  .licenseLink:hover,
  .licenseLink:focus-visible {
    color: var(--text);
  }

  .detailCard--error {
    border-color: rgba(255, 151, 168, 0.24);
    background: #201018;
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

  .detailCopy,
  .stateCopy {
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.5;
  }

  .detailGrid {
    display: grid;
    gap: 8px;
  }

  .pill,
  .button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-height: 32px;
    padding: 0 11px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: #121923;
    color: var(--text);
    white-space: nowrap;
  }

  .field {
    width: 100%;
    min-height: 32px;
    padding: 0 11px;
    border: 1px solid var(--line);
    border-radius: 999px;
    color: var(--text);
    background: #121923;
  }

  .field:focus {
    border-color: var(--line-strong);
    outline: none;
  }

  .pill {
    color: var(--muted);
    font-size: 0.7rem;
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

  .tableWidget[data-pointer-active="true"] .statusDot {
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

  .button {
    cursor: pointer;
    color: var(--text);
    background: #121923;
    transition: border-color 140ms ease, background 140ms ease, transform 140ms ease;
  }

  .button--ghost {
    border-color: transparent;
    color: var(--muted);
    background: #121923;
  }

  .button:hover:not(:disabled) {
    border-color: var(--line-strong);
    background: #18202d;
  }

  .button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
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

  .table {
    display: inline-table;
    width: max-content;
    border-collapse: collapse;
    border-spacing: 0;
    table-layout: auto;
  }

  .table thead {
    position: sticky;
    top: 0;
    z-index: 1;
    background: rgba(12, 15, 20, 0.95);
  }

  .table thead th {
    padding: 8px 8px 6px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: bottom;
  }

  .table tbody tr:first-child td {
    padding-top: 12px;
  }

  .columnName {
    color: var(--text);
    font-size: 0.8rem;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .cell {
    padding: 6px 8px 4px 0;
    vertical-align: top;
  }

  .cellValue {
    display: block;
    color: var(--text);
    font-size: 0.8rem;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .field--select {
    appearance: none;
    min-width: 0;
    padding-right: 28px;
    background-image:
      linear-gradient(45deg, transparent 50%, var(--muted) 50%),
      linear-gradient(135deg, var(--muted) 50%, transparent 50%);
    background-position:
      calc(100% - 14px) 52%,
      calc(100% - 9px) 52%;
    background-size: 5px 5px, 5px 5px;
    background-repeat: no-repeat;
  }

  .emptyState,
  .errorState {
    display: grid;
    align-content: center;
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

  .errorState .stateTitle {
    color: #ffd9df;
  }

  @media (max-width: 900px) {
    body {
      overflow: auto;
    }

    .contentShell {
      grid-template-columns: minmax(0, 1fr);
    }

    .detailsPanel {
      border-left: 0;
      border-top: 1px solid var(--line);
    }
  }
"#;
