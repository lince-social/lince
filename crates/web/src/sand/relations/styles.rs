pub(super) const INLINE_STYLES: [&str; 1] = [STYLE];

const STYLE: &str = r#"
:root {
    color-scheme: dark;
    --bg: #071017;
    --panel: rgba(14, 19, 24, 0.96);
    --panel-soft: rgba(17, 23, 30, 0.96);
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

* {
    box-sizing: border-box;
}

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

body {
    overflow: hidden;
}

body.is-resizing-panels,
body.is-resizing-panels * {
    cursor: ew-resize !important;
    user-select: none !important;
}

button, input, select, textarea {
    font: inherit;
}

[hidden] {
    display: none !important;
}

.app {
    height: 100%;
    padding: 0;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    gap: 0;
}

.panel, .hero {
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
}

.sidePanel {
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

.heroCopy, .heroMeta, .panelHead, .panelToolbar, .panelChips, .toolbarButtons, .sectionHead, .sliderRow, .actionRow, .sidePanelHead, .sidePanelActions {
    display: flex;
    align-items: center;
    gap: 10px;
}

.heroCopy {
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
}

.heroMeta, .panelChips, .toolbarButtons, .panelToolbar, .sidePanelActions {
    flex-wrap: wrap;
}

.sidePanelActions {
    justify-content: flex-end;
}

.heroMeta, .panelChips {
    justify-content: flex-end;
}

.eyebrow, .sectionLabel, .emptyState__eyebrow {
    color: var(--muted);
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
}

.title, .panelTitle, .sectionTitle {
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

.sectionTitle {
    font-size: 0.88rem;
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

.pill--origin {
    color: #d7ecff;
    border-color: rgba(120, 215, 255, 0.22);
    background: rgba(15, 32, 43, 0.72);
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

.button:disabled,
.linkChip[aria-disabled="true"] {
    opacity: 0.55;
    cursor: default;
}

.button--primary {
    border-color: rgba(120, 215, 255, 0.26);
    background: rgba(120, 215, 255, 0.10);
    color: #d8f3ff;
}

.button--danger {
    border-color: rgba(255, 143, 163, 0.28);
    background: rgba(255, 143, 163, 0.10);
    color: #ffe1e8;
}

.button--danger:hover {
    border-color: rgba(255, 143, 163, 0.4);
    background: rgba(255, 143, 163, 0.16);
}

.button--ghost {
    color: var(--muted);
}

.graphPanel {
    min-width: 0;
    min-height: 0;
    display: block;
    position: relative;
    height: 100%;
    padding: 0;
}

.panelHead {
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
}

.panelHeadCopy {
    min-width: 0;
}

.panelToolbar {
    justify-content: flex-end;
    min-width: 0;
}

.graphWorkspace {
    position: relative;
    min-width: 0;
    min-height: 0;
    height: 100%;
}

.graphStage {
    position: relative;
    min-width: 0;
    min-height: 0;
    height: 100%;
    border-radius: 0;
    overflow: hidden;
    border: 0;
    background:
        radial-gradient(circle at 20% 20%, rgba(120, 215, 255, 0.06), transparent 30%),
        linear-gradient(180deg, rgba(8, 12, 18, 0.96), rgba(5, 7, 12, 0.98));
}

.graphOverlay, .graphHud {
    position: absolute;
    z-index: 3;
}

.graphOverlay {
    pointer-events: none;
}

.graphOverlay > *,
.graphHud > * {
    pointer-events: auto;
}

.graphOverlay--title {
    top: 14px;
    left: 14px;
    opacity: 0;
    transition: opacity 140ms ease, transform 140ms ease;
}

.graphOverlay--status {
    top: 14px;
    right: 14px;
}

.graphHud--topRight {
    top: 14px;
    right: 46px;
}

.graphHud .panelToolbar {
    gap: 8px;
}

.graphTitle {
    cursor: help;
    opacity: 1;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.4);
}

.graphOverlay--title:hover,
.graphOverlay--title:focus-within {
    opacity: 1;
}

.graphOverlay--title:hover .graphTitle {
    transform: translateY(-1px);
}

.statusBall {
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

.statusBall:hover {
    transform: scale(1.05);
}

.statusBall[data-open="true"] {
    box-shadow:
        0 0 0 3px rgba(120, 215, 255, 0.12),
        0 8px 24px rgba(0, 0, 0, 0.24);
}

.statusBall[data-tone="live"] {
    border-color: rgba(126, 240, 198, 0.5);
    background:
        radial-gradient(circle at 35% 30%, rgba(255, 255, 255, 0.68), transparent 34%),
        rgba(37, 194, 106, 0.98);
    box-shadow:
        0 0 0 1px rgba(18, 58, 34, 0.56),
        0 0 24px rgba(37, 194, 106, 0.28);
}

.statusBall[data-tone="status"] {
    border-color: rgba(120, 215, 255, 0.36);
    background:
        radial-gradient(circle at 35% 30%, rgba(255, 255, 255, 0.58), transparent 34%),
        rgba(84, 146, 255, 0.96);
    box-shadow:
        0 0 0 1px rgba(18, 43, 82, 0.5),
        0 0 22px rgba(84, 146, 255, 0.24);
}

.statusBall[data-tone="error"] {
    border-color: rgba(255, 143, 163, 0.36);
    background:
        radial-gradient(circle at 35% 30%, rgba(255, 255, 255, 0.58), transparent 34%),
        rgba(255, 99, 123, 0.96);
    box-shadow:
        0 0 0 1px rgba(74, 19, 28, 0.5),
        0 0 22px rgba(255, 99, 123, 0.24);
}

.statusBall[data-tone="neutral"] {
    border-color: rgba(255, 255, 255, 0.14);
    background:
        radial-gradient(circle at 35% 30%, rgba(255, 255, 255, 0.45), transparent 34%),
        rgba(102, 112, 122, 0.95);
}

.pillRow {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
}

#graph {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
    touch-action: none;
}

#graph.is-dragging {
    cursor: grabbing;
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

.graphPanel .button--ghost,
.graphPanel .button {
    backdrop-filter: blur(12px);
}

.sidePanel {
    position: absolute;
    top: 62px;
    right: 0;
    bottom: 0;
    width: min(380px, calc(100% * 0.47));
    max-width: 47%;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 14px;
    padding: 14px;
    overflow: hidden;
    background: linear-gradient(180deg, rgba(14, 19, 24, 0.98), rgba(6, 10, 14, 0.98));
    backdrop-filter: blur(14px);
}

.sidePanel--controls {
    left: 0;
    right: auto;
}

.sidePanel--record {
    right: 0;
    left: auto;
}

.sidePanelResizer {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 12px;
    z-index: 4;
    cursor: ew-resize;
    touch-action: none;
}

.sidePanelResizer::after {
    content: "";
    position: absolute;
    top: 16px;
    bottom: 16px;
    width: 2px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    opacity: 0;
    transition: opacity 120ms ease, background 120ms ease;
}

.sidePanelResizer:hover::after,
body.is-resizing-panels .sidePanelResizer::after {
    opacity: 1;
}

.sidePanelResizer--controls {
    right: 0;
}

.sidePanelResizer--controls::after {
    right: 5px;
}

.sidePanelResizer--record {
    left: 0;
}

.sidePanelResizer--record::after {
    left: 5px;
}

.sidePanelHead {
    justify-content: space-between;
    align-items: flex-start;
}

.sideSection {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: rgba(255, 255, 255, 0.02);
    position: relative;
    overflow: hidden;
    isolation: isolate;
}

.sidePanelBody {
    min-width: 0;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-right: 4px;
}

.sidePanelBody > .sideSection {
    flex: 0 0 auto;
}

.sidePanelBody--create {
    padding-right: 8px;
    padding-bottom: 16px;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
}

.sidePanelBody--record {
    padding-right: 8px;
}

.sideSection--create {
    min-height: max-content;
    overflow: visible;
}

#details {
    flex: 0 0 auto;
}

#editor {
    flex: 0 0 auto;
}

#selection-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
}

.section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
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

.textarea {
    width: 100%;
    min-height: 104px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
    padding: 10px;
    resize: vertical;
}

.textarea--compact {
    min-height: 58px;
}

.recordId {
    color: var(--soft);
    font-family: var(--mono);
    font-size: 0.78rem;
    letter-spacing: 0.04em;
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

.recordEditor {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
}

.fieldGroup {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
}

.fieldGroup--id {
    gap: 6px;
}

.needChoiceList {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: 260px;
    padding-right: 6px;
    overflow: auto;
    min-width: 0;
}

.needChoiceList--compact {
    max-height: 168px;
}

.relationRowList {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
}

.needChoice {
    width: 100%;
    text-align: left;
    cursor: pointer;
}

.needChoice:hover {
    border-color: var(--line-strong);
    background: rgba(255, 255, 255, 0.06);
}

.needChoice.is-selected {
    border-color: rgba(120, 215, 255, 0.36);
    background: rgba(120, 215, 255, 0.10);
}

.relationRow {
    position: relative;
    display: flex;
    align-items: stretch;
    gap: 10px;
    min-width: 0;
    min-height: 52px;
    padding: 11px 44px 11px 12px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
    overflow: hidden;
}

.relationRow:hover,
.relationRow:focus-within {
    border-color: var(--line-strong);
    background: rgba(255, 255, 255, 0.06);
}

.relationRow.is-selected {
    border-color: rgba(120, 215, 255, 0.36);
    background: rgba(120, 215, 255, 0.10);
}

.relationRow__body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    flex: 1;
}

.relationRow__head {
    font-size: 0.8rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.relationRow__meta {
    color: var(--muted);
    font-size: 0.72rem;
    line-height: 1.45;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: normal;
}

.relationRow__action {
    position: absolute;
    top: 50%;
    right: 8px;
    width: 24px;
    height: 24px;
    min-height: 24px;
    padding: 0;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text);
    line-height: 1;
    transform: translateY(-50%) scale(0.96);
    opacity: 0;
    pointer-events: none;
    transition: opacity 120ms ease, transform 120ms ease, border-color 120ms ease, background 120ms ease;
}

.relationRow:hover .relationRow__action,
.relationRow:focus-within .relationRow__action {
    opacity: 1;
    transform: translateY(-50%) scale(1);
    pointer-events: auto;
}

.relationRow__action:hover {
    border-color: var(--line-strong);
    background: rgba(255, 255, 255, 0.08);
}

.relationRow__action--add {
    color: #d8f3ff;
    border-color: rgba(120, 215, 255, 0.28);
    background: rgba(120, 215, 255, 0.08);
}

.relationRow__action--remove {
    color: #ffe1e8;
    border-color: rgba(255, 143, 163, 0.28);
    background: rgba(255, 143, 163, 0.08);
}

.needChoice__head {
    font-size: 0.8rem;
    font-weight: 600;
}

.needChoice__meta {
    color: var(--muted);
    font-size: 0.72rem;
    line-height: 1.45;
}

@media (max-width: 980px) {
    body {
        overflow: auto;
    }

    .app {
        height: auto;
        min-height: 100%;
    }

    .graphPanel {
        min-height: 72vh;
    }

    .panelToolbar {
        justify-content: flex-start;
    }

    .sidePanel {
        inset: 8px;
        width: auto;
    }
}
"#;
