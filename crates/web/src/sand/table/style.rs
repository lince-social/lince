pub(super) const INLINE_STYLES: [&str; 1] = [STYLE];

const STYLE: &str = r#"
  :root {
    color-scheme: dark;
    --bg: #0c1117;
    --panel: rgba(17, 22, 29, 0.92);
    --panel-soft: rgba(14, 19, 25, 0.96);
    --line: rgba(255, 255, 255, 0.08);
    --line-strong: rgba(255, 255, 255, 0.16);
    --cell-line: rgba(255, 255, 255, 0.045);
    --cell-line-focus: rgba(255, 255, 255, 0.12);
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
    top: 12px;
    right: 0;
    bottom: 12px;
    width: min(360px, 100%);
    padding: 12px 12px 14px;
    border-left: 1px solid var(--line);
    background: #0b1017;
    box-shadow: -28px 0 48px rgba(0, 0, 0, 0.35);
    opacity: 1;
    z-index: 2;
    overflow: auto;
  }

  .createPanel {
    min-height: 0;
    position: absolute;
    top: 12px;
    right: 0;
    bottom: 12px;
    width: min(360px, 100%);
    border-left: 1px solid var(--line);
    background: #0b1017;
    box-shadow: -28px 0 48px rgba(0, 0, 0, 0.35);
    z-index: 3;
    overflow: auto;
  }

  .contentShell[data-create-open="false"] .createPanel {
    display: none;
  }

  .contentShell[data-create-open="true"] .createPanel {
    display: block;
  }

  .contentShell[data-info-open="false"] .detailsPanel {
    display: none;
  }

  .contentShell[data-info-open="true"] .detailsPanel {
    display: block;
  }

  .createForm {
    display: grid;
    gap: 10px;
    padding: 12px;
  }

  .panelHeader {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 2px;
  }

  .panelTitle {
    color: var(--text);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .createFields {
    display: grid;
    gap: 10px;
  }

  .createField {
    display: grid;
    gap: 4px;
  }

  .fieldLabel {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
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
  }

  .tablePanel::-webkit-scrollbar {
    width: 0;
    height: 0;
  }

  .tablePanel[data-scrolling="true"] {
    scrollbar-width: thin;
    -ms-overflow-style: auto;
  }

  .tablePanel[data-scrolling="true"]::-webkit-scrollbar {
    width: 10px;
    height: 10px;
  }

  .tableFrame {
    min-height: 0;
    min-width: 0;
    display: inline-block;
    width: max-content;
  }

  .tablePanel[data-mode="helix"] tr[data-row-focused="true"] td:first-child {
    position: relative;
  }

  .tablePanel[data-mode="helix"] tr[data-row-focused="true"] td:first-child::before {
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

  .detailCard--setting {
    gap: 10px;
    align-items: start;
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

  .field--textarea {
    min-height: 100px;
    padding-top: 9px;
    padding-bottom: 9px;
    border-radius: 14px;
    resize: vertical;
    font-family: var(--mono);
    line-height: 1.5;
  }

  .pill {
    color: var(--muted);
    font-size: 0.7rem;
  }

  .status {
    display: inline-flex;
    width: 18px;
    height: 18px;
    min-width: 18px;
    min-height: 18px;
    padding: 0;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    background:
      radial-gradient(circle at 35% 30%, rgba(255, 255, 255, 0.55), transparent 34%),
      rgba(102, 112, 122, 0.95);
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.22),
      0 8px 24px rgba(0, 0, 0, 0.24);
    transition: transform 140ms ease, box-shadow 140ms ease, border-color 140ms ease, background 140ms ease;
  }

  .status:hover {
    transform: scale(1.05);
  }

  .status[data-tone="live"] {
    border-color: rgba(141, 240, 185, 0.5);
    background:
      radial-gradient(circle at 35% 30%, rgba(255, 255, 255, 0.68), transparent 34%),
      rgba(37, 194, 106, 0.98);
    box-shadow:
      0 0 0 1px rgba(18, 58, 34, 0.56),
      0 0 24px rgba(37, 194, 106, 0.28);
  }

  .status[data-tone="loading"] {
    border-color: rgba(126, 240, 198, 0.48);
    background:
      radial-gradient(circle at 35% 30%, rgba(255, 255, 255, 0.64), transparent 34%),
      rgba(34, 203, 196, 0.96);
    box-shadow:
      0 0 0 1px rgba(16, 76, 72, 0.52),
      0 0 24px rgba(34, 203, 196, 0.28);
  }

  .status[data-tone="error"] {
    border-color: rgba(255, 151, 168, 0.36);
    background:
      radial-gradient(circle at 35% 30%, rgba(255, 255, 255, 0.58), transparent 34%),
      rgba(255, 99, 123, 0.96);
    box-shadow:
      0 0 0 1px rgba(74, 19, 28, 0.5),
      0 0 22px rgba(255, 99, 123, 0.24);
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

  .button--ghost:hover:not(:disabled),
  .button--ghost:focus-visible {
    color: var(--text);
    background: rgba(255, 255, 255, 0.04);
  }

  .button:hover:not(:disabled) {
    border-color: var(--line-strong);
    background: #18202d;
  }

  .button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .button--accent {
    color: #dfebff;
    border-color: rgba(138, 180, 255, 0.24);
    background: #22314d;
  }

  .button--danger {
    color: #ffe0e6;
    border-color: rgba(255, 151, 168, 0.24);
    background: #261018;
  }

  .button--danger:hover:not(:disabled),
  .button--danger:focus-visible {
    border-color: rgba(255, 151, 168, 0.38);
    background: #34131f;
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
    border: 1px solid var(--cell-line);
    text-align: left;
    vertical-align: bottom;
    background: rgba(12, 15, 20, 0.95);
  }

  .columnName {
    color: var(--text);
    font-size: 0.8rem;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .cell {
    padding: 7px 8px 5px;
    border: 1px solid var(--cell-line);
    vertical-align: top;
    background: rgba(255, 255, 255, 0.01);
    transition: box-shadow 120ms ease, border-color 120ms ease;
  }

  .cell--id {
    position: relative;
  }

  .cell[data-focused-cell="true"] {
    border-color: var(--cell-line-focus);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.02);
  }

  .cellValue--id {
    padding-right: 74px;
  }

  .cellValue {
    display: block;
    color: var(--text);
    font-size: 0.8rem;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .cellEditor {
    appearance: none;
    display: block;
    min-width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    outline: none;
    resize: none;
    overflow: auto;
    background: transparent;
    box-shadow: none;
    color: inherit;
    caret-color: var(--text);
    font: inherit;
    line-height: inherit;
    white-space: pre-wrap;
    scrollbar-width: none;
  }

  .cellEditor::-webkit-scrollbar {
    width: 0;
    height: 0;
  }

  .cellValue[data-editing-cell="true"] {
    outline: none;
    cursor: text;
  }

  .rowDeleteButton {
    position: absolute;
    top: 4px;
    right: 4px;
    min-height: 24px;
    padding: 0 9px;
    border-radius: 999px;
    font-size: 0.68rem;
    line-height: 1;
    opacity: 0;
    transform: translateY(-2px);
    pointer-events: none;
    z-index: 1;
  }

  .cell--id:hover .rowDeleteButton,
  .cell--id:focus-within .rowDeleteButton {
    opacity: 1;
    transform: translateY(0);
    pointer-events: auto;
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

  .toastLayer {
    position: fixed;
    left: 12px;
    bottom: 12px;
    z-index: 20;
    display: grid;
    gap: 8px;
    pointer-events: none;
  }

  .toast {
    min-width: min(320px, calc(100vw - 24px));
    max-width: min(420px, calc(100vw - 24px));
    padding: 10px 12px;
    border: 1px solid rgba(255, 151, 168, 0.28);
    border-radius: 12px;
    background: rgba(32, 16, 23, 0.96);
    color: #ffd9df;
    box-shadow: 0 16px 34px rgba(0, 0, 0, 0.32);
    font-size: 0.78rem;
    line-height: 1.45;
    pointer-events: auto;
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
