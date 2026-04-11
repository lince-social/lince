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
    min-height: 100vh;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 0;
  }

  .topLine {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px 8px;
  }

  .topLineTitle {
    font-size: 0.92rem;
    font-weight: 600;
    letter-spacing: -0.02em;
  }

  .topLineActions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .contentShell {
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(280px, 360px);
    gap: 0;
  }

  .detailsPanel {
    min-height: 0;
    padding: 12px 12px 14px;
    border-left: 1px solid var(--line);
    background: rgba(255, 255, 255, 0.014);
    overflow: auto;
  }

  .tablePanel {
    min-width: 0;
    min-height: 0;
    overflow: auto;
  }

  .tableFrame {
    min-height: 100%;
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
    background: rgba(255, 255, 255, 0.03);
  }

  .detailCard--error {
    border-color: rgba(255, 151, 168, 0.24);
    background: rgba(74, 28, 42, 0.16);
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
  .status,
  .button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-height: 32px;
    padding: 0 11px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
    white-space: nowrap;
  }

  .pill {
    color: var(--muted);
    font-size: 0.7rem;
  }

  .status {
    color: var(--muted);
    font-size: 0.7rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .status[data-tone="live"] {
    color: #daf7e6;
    border-color: rgba(141, 240, 185, 0.22);
    background: rgba(19, 46, 32, 0.72);
  }

  .status[data-tone="loading"] {
    color: #f7e6bf;
    border-color: rgba(243, 199, 123, 0.22);
    background: rgba(44, 34, 15, 0.72);
  }

  .status[data-tone="error"] {
    color: #ffd9df;
    border-color: rgba(255, 151, 168, 0.22);
    background: rgba(57, 21, 31, 0.72);
  }

  .button {
    cursor: pointer;
    color: var(--text);
    background: rgba(255, 255, 255, 0.03);
    transition: border-color 140ms ease, background 140ms ease, transform 140ms ease;
  }

  .button:hover:not(:disabled) {
    border-color: var(--line-strong);
    background: rgba(255, 255, 255, 0.06);
  }

  .button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .button--accent {
    color: #dfebff;
    border-color: rgba(138, 180, 255, 0.24);
    background: rgba(47, 66, 104, 0.28);
  }

  .codeBlock {
    margin: 0;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: rgba(0, 0, 0, 0.22);
    color: #d7e3f2;
    font-family: var(--mono);
    font-size: 0.72rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    overflow: auto;
  }

  .table {
    width: 100%;
    border-collapse: collapse;
    border-spacing: 0;
  }

  .table thead th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 8px 8px 6px;
    border-bottom: 1px solid var(--line);
    background: rgba(12, 15, 20, 0.95);
    text-align: left;
    vertical-align: bottom;
  }

  .rowHeaderCell {
    width: 220px;
  }

  .columnName {
    color: var(--text);
    font-size: 0.8rem;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .columnKey,
  .rowLabel {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.66rem;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .rowHeaderCell {
    padding: 6px 8px 4px 0;
    vertical-align: top;
  }

  .rowSummary {
    display: grid;
    gap: 4px;
  }

  .rowIndex {
    color: var(--text);
    font-size: 0.8rem;
    font-weight: 700;
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
