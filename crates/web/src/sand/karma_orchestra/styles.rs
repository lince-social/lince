pub(crate) const INLINE_STYLES: [&str; 1] = [STYLE];

const STYLE: &str = r#"
:root {
    color-scheme: dark;
    --bg: #11100e;
    --panel: rgba(25, 23, 19, 0.94);
    --line: rgba(255,255,255,0.14);
    --text: #f1ece2;
    --muted: #b8ae9c;
    --accent: #e7b75f;
    --danger: #d97757;
}

* { box-sizing: border-box; }
html, body { height: 100%; margin: 0; overflow: hidden; }
body {
    background: linear-gradient(135deg, #15120f, #090807);
    color: var(--text);
    font-family: ui-serif, Georgia, "Times New Roman", serif;
}
button, input { font: inherit; }
[hidden] { display: none !important; }

.karmaOrchestra, .stageShell, .stage { width: 100%; height: 100%; }
.stage { position: relative; overflow: hidden; }
.graph { width: 100%; height: 100%; display: block; color: rgba(241,236,226,.72); }

.topHud {
    position: absolute;
    z-index: 3;
    top: 14px;
    left: 14px;
    right: 14px;
    display: flex;
    justify-content: space-between;
    gap: 12px;
    pointer-events: none;
}
.title { margin: 0; font-size: 1.05rem; letter-spacing: .08em; text-transform: uppercase; }
.pills { display: flex; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
.pill, .button, .viewButton, .segment {
    border: 1px solid var(--line);
    background: rgba(255,255,255,.05);
    color: var(--text);
    border-radius: 999px;
    min-height: 32px;
    padding: 0 12px;
}
.pill { color: var(--muted); display: inline-flex; align-items: center; font-size: .74rem; }
.button, .viewButton, .segment { cursor: pointer; }
.primary { background: rgba(231,183,95,.18); border-color: rgba(231,183,95,.5); }

.stateBall {
    position: absolute;
    z-index: 5;
    top: 58px;
    left: 16px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid rgba(255,255,255,.65);
    background: linear-gradient(135deg, #2a251d, #e7b75f);
    cursor: pointer;
}
.viewButton {
    position: absolute;
    z-index: 5;
    right: 18px;
    bottom: 18px;
}

.emptyState {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
}
.emptyCard, .viewModal, .adjustments {
    border: 1px solid var(--line);
    border-radius: 18px;
    background: var(--panel);
    box-shadow: 0 24px 70px rgba(0,0,0,.38);
    backdrop-filter: blur(14px);
}
.emptyCard { width: min(380px, calc(100% - 40px)); padding: 24px; text-align: center; }
.emptyCard h2 { margin: 6px 0 8px; }
.emptyCard p { margin: 0; color: var(--muted); line-height: 1.45; }
.eyebrow { color: var(--accent); font: 700 .68rem ui-sans-serif, system-ui; letter-spacing: .18em; text-transform: uppercase; }

.viewModal, .adjustments {
    position: absolute;
    z-index: 8;
    right: 18px;
    bottom: 60px;
    width: min(360px, calc(100% - 36px));
    padding: 14px;
}
.adjustments { left: 18px; right: auto; bottom: 18px; }
.modalHead { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; margin-bottom: 12px; }
.modalHead h2 { margin: 2px 0 0; font-size: 1rem; }
.field { display: grid; gap: 7px; margin-top: 10px; color: var(--muted); font-size: .78rem; }
.physicsHead { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.sliderRow { display: flex; justify-content: space-between; align-items: center; gap: 10px; color: var(--text); flex-wrap: nowrap; }
.sliderLabel { font-size: .76rem; color: var(--muted); }
.sliderValue { font-family: var(--mono); font-size: .72rem; color: var(--soft); }
.slider {
    width: 100%;
    appearance: none;
    height: 4px;
    border-radius: 999px;
    background: rgba(255,255,255,.12);
    outline: none;
}
.slider::-webkit-slider-thumb {
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #e6edf3;
    border: 2px solid rgba(0,0,0,.2);
    cursor: pointer;
}
.slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #e6edf3;
    border: 2px solid rgba(0,0,0,.2);
    cursor: pointer;
}
.colorGrid { display: grid; gap: 8px; }
.colorRow { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
.input {
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(0,0,0,.22);
    color: var(--text);
    min-height: 36px;
    padding: 0 10px;
}
.colorRow .input[type="color"] {
    width: 52px;
    min-height: 32px;
    padding: 0;
    border: 0;
    background: transparent;
}
.viewList { display: grid; gap: 8px; max-height: 240px; overflow: auto; }
.viewRow { width: 100%; text-align: left; border-radius: 12px; padding: 10px; }
.muted { color: var(--muted); font-size: .78rem; }
.segmented { display: grid; grid-template-columns: repeat(2, 1fr); gap: 7px; }
.segment.is-active { background: rgba(231,183,95,.22); border-color: rgba(231,183,95,.58); }
.toggleGrid { display: grid; gap: 8px; }
.toggleRow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
}
.toggleRow input { width: 18px; height: 18px; accent-color: var(--accent); }
.repulsionValue { width: 92px; flex: 0 0 92px; }
.summaryGrid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; margin-top: 14px; }
.summaryGrid div { border: 1px solid var(--line); border-radius: 12px; padding: 9px; display: grid; gap: 3px; }
.summaryGrid span { color: var(--muted); font-size: .72rem; }

.nodeLabel { pointer-events: none; overflow: visible; }
.nodeCard {
    position: relative;
    width: 100%;
    height: 100%;
    display: grid;
    align-content: center;
    justify-items: start;
    gap: 1px;
    padding: 10px 12px 9px 10px;
    border-radius: 2px;
    border: 1px solid currentColor;
    background: rgba(25,23,19,.96);
    box-shadow: 0 10px 24px rgba(0,0,0,.18);
    font: 9px ui-sans-serif, system-ui;
    text-align: left;
    overflow: hidden;
}
.nodeCard .human {
    font-weight: 700;
    color: var(--text);
    font-size: 8px;
    white-space: nowrap;
    line-height: 1;
}
.nodeCard .meta {
    color: var(--muted);
    font-size: 6.5px;
    white-space: nowrap;
    line-height: 1;
}
.nodeBadge {
    position: absolute;
    top: 2px;
    right: 3px;
    width: 12px;
    height: 12px;
    display: grid;
    place-items: center;
    pointer-events: none;
}
.nodeBadge svg {
    display: block;
    width: 100%;
    height: 100%;
}
.nodeBadgeFill { fill: currentColor; }
.nodeBadgeStroke { fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linejoin: round; }
.link { fill: none; stroke: rgba(255,255,255,.84); stroke-width: 1.6; }
.link.fulfillment { stroke-dasharray: 4 4; }
.link.inactive { stroke: rgba(155,155,155,.45); }
.link.inactive.fulfillment { stroke: rgba(155,155,155,.45); }
.link.loop { stroke-width: 2.2; }
.arrowHead { stroke: none; }
"#;
