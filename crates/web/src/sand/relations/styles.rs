pub(super) const INLINE_STYLES: [&str; 1] = [STYLE];

const STYLE: &str = r#"
:root {
    color-scheme: dark;
    --bg: #071017;
    --panel: rgba(14, 19, 24, 0.96);
    --panel-alt: rgba(17, 23, 30, 0.96);
    --line: rgba(255, 255, 255, 0.08);
    --line-strong: rgba(255, 255, 255, 0.16);
    --text: #e6edf3;
    --muted: #9aa8b6;
    --soft: #c8d1db;
    --accent: #78d7ff;
    --accent-2: #7ef0c6;
    --warn: #f2bb78;
    --danger: #ff8fa3;
    --shadow: 0 20px 55px rgba(0, 0, 0, 0.28);
    --mono: "SFMono-Regular", "IBM Plex Mono", Consolas, monospace;
}

* { box-sizing: border-box; }

html, body {
    height: 100%;
    margin: 0;
    background:
        radial-gradient(circle at top left, rgba(120, 215, 255, 0.10), transparent 30%),
        radial-gradient(circle at top right, rgba(126, 240, 198, 0.08), transparent 26%),
        linear-gradient(180deg, #09131a 0%, #05080d 100%);
    color: var(--text);
    font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

body { overflow: hidden; }

button, input, select {
    font: inherit;
}

[hidden] {
    display: none !important;
}

.app {
    height: 100%;
    padding: 10px;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 10px;
}

.panel, .hero {
    border: 1px solid var(--line);
    border-radius: 16px;
    background: linear-gradient(180deg, var(--panel), rgba(8, 12, 16, 0.98));
    box-shadow: var(--shadow);
}

.hero {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 15px;
    align-items: flex-start;
}

.heroCopy, .heroMeta, .panelHead, .sectionHead, .sliderRow, .actionRow {
    display: flex;
    align-items: center;
    gap: 10px;
}

.heroCopy {
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
}

.heroMeta {
    flex-wrap: wrap;
    justify-content: flex-end;
}

.eyebrow, .sectionLabel, .emptyState__eyebrow {
    color: var(--muted);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
}

.title, .panelTitle {
    margin: 0;
    letter-spacing: -0.03em;
}

.title {
    font-size: 1.08rem;
    font-weight: 700;
}

.panelTitle {
    font-size: 0.96rem;
    font-weight: 700;
}

.copy, .mutedCopy, .selectionEmpty p, .emptyState__copy {
    margin: 0;
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.45;
}

.pill, .linkChip, .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 32px;
    padding: 0 11px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
    text-decoration: none;
    white-space: nowrap;
}

.pill {
    font-size: 0.7rem;
    color: var(--muted);
}

.pill--mode {
    color: #d8f5e8;
    border-color: rgba(126, 240, 198, 0.22);
    background: rgba(20, 44, 34, 0.72);
}

.pill--status {
    color: #d7ecff;
    border-color: rgba(120, 215, 255, 0.22);
    background: rgba(15, 32, 43, 0.72);
}

.linkChip, .button {
    cursor: pointer;
    transition: border-color 120ms ease, background 120ms ease, transform 120ms ease;
}

.linkChip:hover, .button:hover {
    border-color: var(--line-strong);
    background: rgba(255, 255, 255, 0.06);
}

.button--primary {
    border-color: rgba(120, 215, 255, 0.26);
    background: rgba(120, 215, 255, 0.10);
    color: #d8f3ff;
}

.button--ghost {
    color: var(--muted);
}

.layout {
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: 10px;
}

.graphPanel, .editorPanel, .detailPanel {
    padding: 12px;
    min-width: 0;
    min-height: 0;
}

.graphPanel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 10px;
}

.panelHead {
    justify-content: space-between;
    flex-wrap: wrap;
}

.panelChips {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
}

.graphStage {
    position: relative;
    min-height: 0;
    min-width: 0;
    border-radius: 14px;
    overflow: hidden;
    border: 1px solid var(--line);
    background:
        radial-gradient(circle at 20% 20%, rgba(120, 215, 255, 0.06), transparent 30%),
        linear-gradient(180deg, rgba(8, 12, 18, 0.96), rgba(5, 7, 12, 0.98));
}

#graph {
    display: block;
    width: 100%;
    height: 100%;
    cursor: crosshair;
}

.emptyState {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
}

.emptyState__box {
    padding: 18px 20px;
    border: 1px dashed var(--line-strong);
    border-radius: 16px;
    background: rgba(255, 255, 255, 0.02);
    text-align: center;
    max-width: 32ch;
}

.emptyState__title {
    margin: 6px 0 4px;
    font-size: 0.98rem;
    letter-spacing: -0.03em;
}

.sidebar {
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: minmax(240px, 0.92fr) minmax(320px, 1.08fr);
    gap: 10px;
    align-content: stretch;
}

.editorPanel {
    display: grid;
    gap: 14px;
    min-height: 0;
    overflow: auto;
}

.detailPanel {
    order: -1;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 14px;
    min-height: 0;
    overflow: auto;
}

#selection-content {
    display: grid;
    gap: 12px;
    align-content: start;
}

.section {
    display: grid;
    gap: 10px;
}

.sectionTitle {
    margin: 0;
    font-size: 0.88rem;
    letter-spacing: -0.02em;
}

.fieldLabel, .sliderLabel {
    color: var(--soft);
    font-size: 0.73rem;
    font-weight: 600;
}

.input, .select {
    width: 100%;
    min-height: 36px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
    padding: 0 10px;
}

.codeBlock {
    margin: 0;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(0, 0, 0, 0.26);
    color: var(--muted);
    font-family: var(--mono);
    font-size: 0.72rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
}

.checkList, .chipList {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
}

.chipButton {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: rgba(120, 215, 255, 0.06);
    color: var(--text);
    font-size: 0.74rem;
    cursor: pointer;
}

.chipButton:hover {
    border-color: var(--line-strong);
    background: rgba(120, 215, 255, 0.10);
}

.checkItem {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 7px 9px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.03);
    font-size: 0.74rem;
    color: var(--text);
}

.checkItem input {
    margin: 0;
}

.sliderRow {
    justify-content: space-between;
}

.sliderValue {
    color: var(--accent);
    font-family: var(--mono);
    font-size: 0.72rem;
}

.slider {
    width: 100%;
    accent-color: var(--accent);
}

.actionRow {
    justify-content: flex-start;
    flex-wrap: wrap;
}

.actionRow--split {
    justify-content: space-between;
}

.selectionEmpty {
    display: grid;
    gap: 8px;
}

.selectionEmpty p {
    max-width: 28ch;
}

@media (max-width: 980px) {
    body {
        overflow: auto;
    }

    .app {
        height: auto;
        min-height: 100%;
    }

    .layout {
        grid-template-columns: minmax(0, 1fr);
    }

    .sidebar {
        grid-template-rows: minmax(240px, auto) minmax(320px, auto);
    }
}
"#;
