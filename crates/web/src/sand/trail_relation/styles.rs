pub(crate) const INLINE_STYLES: &[&str] = &[r#"
    :root {
        color-scheme: dark;
        --bg: #090c12;
        --bgGlow: rgba(22, 163, 74, 0.12);
        --panel: rgba(15, 20, 29, 0.92);
        --panelSoft: rgba(10, 14, 22, 0.96);
        --ink: #edf2f7;
        --muted: #98a2b3;
        --line: rgba(148, 163, 184, 0.2);
        --lineStrong: rgba(52, 211, 153, 0.4);
        --accent: #1f8a70;
        --accentStrong: #29b585;
        --warn: #f59e0b;
        --shadow: 0 24px 60px rgba(0, 0, 0, 0.35);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    }

    * { box-sizing: border-box; }

    html, body {
        margin: 0;
        min-height: 100%;
        background:
            radial-gradient(circle at top left, var(--bgGlow), transparent 28%),
            radial-gradient(circle at bottom right, rgba(56, 189, 248, 0.08), transparent 34%),
            linear-gradient(180deg, #07090f 0%, var(--bg) 100%);
        color: var(--ink);
    }

    body { padding: 16px; }

    .trailShell {
        display: grid;
        gap: 14px;
    }

    .trailHeader,
    .panel {
        border: 1px solid var(--line);
        border-radius: 16px;
        background: var(--panel);
        box-shadow: var(--shadow);
        padding: 16px;
        backdrop-filter: blur(10px);
    }

    .trailHeader {
        display: flex;
        justify-content: space-between;
        gap: 12px;
        align-items: flex-start;
    }

    h1, h2 {
        margin: 0 0 8px;
        color: var(--ink);
    }

    h1 { font-size: 1.35rem; }
    h2 { font-size: 1rem; }

    .small,
    .status,
    .warning {
        margin: 0;
        color: var(--muted);
    }

    .warning { color: var(--warn); }

    .discoverLayout {
        display: grid;
        grid-template-columns: minmax(240px, 320px) 1fr;
        gap: 16px;
        align-items: start;
    }

    .discoverFilters {
        display: grid;
        gap: 12px;
    }

    .discoverResults {
        display: grid;
        gap: 10px;
        min-width: 0;
    }

    .grid {
        display: grid;
        gap: 12px;
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    }

    .field,
    .choiceFieldset {
        display: grid;
        gap: 6px;
        font-size: 0.92rem;
        min-width: 0;
    }

    .fieldWide {
        grid-column: 1 / -1;
    }

    .fieldInput,
    .button {
        min-height: 40px;
        border-radius: 10px;
        border: 1px solid var(--line);
        background: rgba(8, 11, 18, 0.96);
        color: var(--ink);
        padding: 9px 11px;
        transition: border-color 140ms ease, background 140ms ease, transform 140ms ease;
    }

    .fieldInput::placeholder {
        color: #7f8a9d;
    }

    .fieldInput:focus,
    .button:focus {
        outline: none;
        border-color: var(--lineStrong);
        box-shadow: 0 0 0 1px rgba(41, 181, 133, 0.24);
    }

    .button {
        cursor: pointer;
        font-weight: 600;
        background: rgba(17, 24, 39, 0.92);
    }

    .button:hover {
        transform: translateY(-1px);
        border-color: rgba(96, 165, 250, 0.32);
    }

    .button:disabled {
        cursor: not-allowed;
        opacity: 0.55;
        transform: none;
    }

    .buttonAccent {
        background: linear-gradient(180deg, var(--accentStrong) 0%, var(--accent) 100%);
        color: #04130e;
        border-color: rgba(52, 211, 153, 0.34);
    }

    .inlineField {
        display: flex;
        gap: 8px;
    }

    .inlineField .fieldInput {
        flex: 1 1 auto;
    }

    .autocompleteHost {
        position: relative;
    }

    .suggestionPanel {
        position: absolute;
        top: calc(100% + 6px);
        left: 0;
        right: 0;
        z-index: 30;
        display: grid;
        gap: 6px;
        max-height: 220px;
        overflow: auto;
        padding: 8px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: var(--panelSoft);
        box-shadow: var(--shadow);
    }

    .suggestionButton {
        width: 100%;
        text-align: left;
        border-radius: 10px;
        border: 1px solid var(--line);
        background: rgba(17, 24, 39, 0.92);
        color: var(--ink);
        padding: 10px 12px;
        cursor: pointer;
    }

    .suggestionButton:hover {
        border-color: var(--lineStrong);
        background: rgba(20, 30, 45, 0.96);
    }

    .suggestionMeta {
        display: block;
        margin-top: 4px;
        color: var(--muted);
        font-size: 0.84rem;
    }

    .choiceFieldset {
        border: 1px solid var(--line);
        border-radius: 12px;
        padding: 12px;
        background: rgba(8, 11, 18, 0.7);
    }

    .choiceFieldset legend {
        padding: 0 6px;
        color: var(--ink);
        font-weight: 600;
    }

    .choiceGroup {
        display: flex;
        flex-wrap: wrap;
        gap: 10px;
    }

    .checkboxField {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        padding: 8px 10px;
        border-radius: 999px;
        border: 1px solid var(--line);
        background: rgba(17, 24, 39, 0.82);
        color: var(--ink);
        cursor: pointer;
    }

    .checkboxField input {
        margin: 0;
        accent-color: var(--accentStrong);
    }

    .searchSummary {
        margin-top: 2px;
    }

    .resultListShell {
        max-height: 360px;
        overflow: auto;
        border: 1px solid var(--line);
        border-radius: 14px;
        background: rgba(8, 11, 18, 0.58);
        padding: 10px;
    }

    .resultList,
    .tree {
        display: grid;
        gap: 10px;
    }

    .resultCard,
    .trailNode {
        border: 1px solid var(--line);
        border-radius: 12px;
        padding: 12px;
        background: rgba(8, 11, 18, 0.78);
    }

    .resultCard {
        cursor: pointer;
    }

    .resultCard:hover {
        border-color: rgba(96, 165, 250, 0.32);
    }

    .resultCard.isSelected {
        border-color: var(--lineStrong);
        box-shadow: inset 0 0 0 1px rgba(41, 181, 133, 0.22);
    }

    .resultExcerpt {
        margin: 6px 0 0;
        color: #c8d1df;
        font-size: 0.9rem;
        line-height: 1.4;
    }

    .resultMeta,
    .nodeMeta {
        color: var(--muted);
        font-size: 0.88rem;
        margin-top: 6px;
    }

    .row {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        align-items: center;
        justify-content: space-between;
    }

    .rowActions {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        align-items: center;
    }

    .pill {
        border: 1px solid var(--line);
        border-radius: 999px;
        padding: 3px 8px;
        font-size: 0.82rem;
        color: var(--muted);
        background: rgba(17, 24, 39, 0.92);
    }

    .trailNode {
        border-left: 4px solid rgba(41, 181, 133, 0.36);
    }

    .emptyState {
        padding: 18px 12px;
        text-align: center;
    }

    @media (max-width: 860px) {
        .discoverLayout {
            grid-template-columns: 1fr;
        }
    }

    @media (max-width: 720px) {
        body { padding: 12px; }
        .trailHeader, .panel { padding: 14px; }
        .rowActions { width: 100%; }
        .rowActions .button { flex: 1 1 auto; }
        .choiceGroup { flex-direction: column; }
        .checkboxField { width: 100%; justify-content: flex-start; }
        .inlineField { flex-direction: column; }
        .resultListShell { max-height: 280px; }
    }
"#];
