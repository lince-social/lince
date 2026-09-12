pub(super) const CSS: &str = r#"
  :root {
    color-scheme: dark;
    --bg: #080a0c;
    --panel: #101419;
    --line: #2a3038;
    --line-strong: #4a535e;
    --text: #edf1f5;
    --muted: #9ca7b3;
    --accent: #e8693f;
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; height: 100%; background: var(--bg); color: var(--text); }
  body { padding: 8px; font: 13px/1.4 system-ui, -apple-system, sans-serif; }
  [hidden] { display: none !important; }
  .app { height: calc(100vh - 16px); display: grid; grid-template-rows: auto minmax(0, 1fr); gap: 8px; }
  .topbar, .game { border: 1px solid var(--line); background: var(--panel); border-radius: 6px; }
  .topbar { display: grid; grid-template-columns: minmax(160px, 1fr) auto; gap: 8px 16px; padding: 10px; }
  .identity h1 { margin: 2px 0 0; font-size: 16px; letter-spacing: 0; }
  .identity p { margin: 2px 0 0; color: var(--muted); font-size: 12px; }
  .eyebrow { color: var(--muted); font-size: 10px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
  .notices { display: flex; align-items: flex-start; gap: 12px; }
  .notices a { color: var(--muted); font-size: 12px; text-decoration: none; }
  .notices a:hover { color: var(--text); text-decoration: underline; }
  .controls { grid-column: 1 / -1; display: flex; align-items: flex-start; flex-wrap: wrap; gap: 6px; }
  button, summary {
    min-height: 32px; border: 1px solid var(--line); border-radius: 5px; padding: 0 10px;
    background: #171c22; color: var(--text); font: inherit; cursor: pointer;
  }
  button:hover:not(:disabled), summary:hover { border-color: var(--line-strong); background: #1d232b; }
  button:disabled { opacity: .5; cursor: wait; }
  button.primary { border-color: #a64b2e; background: #6e321f; font-weight: 700; }
  details { position: relative; }
  summary { display: flex; align-items: center; list-style: none; }
  summary::-webkit-details-marker { display: none; }
  pre {
    position: absolute; z-index: 2; top: 30px; left: 0; width: min(560px, calc(100vw - 36px)); max-height: 150px;
    overflow: auto; margin: 4px 0 0; padding: 9px; border: 1px solid var(--line-strong); border-radius: 5px;
    background: #060708; color: var(--muted); white-space: pre-wrap; font: 11px/1.4 ui-monospace, monospace;
  }
  .game { min-height: 0; padding: 8px; display: grid; grid-template-rows: auto minmax(0, 1fr); gap: 8px; }
  .statusbar { display: flex; justify-content: space-between; align-items: center; gap: 10px; color: var(--muted); font-size: 12px; }
  .statusmain { min-width: 0; display: flex; align-items: center; gap: 8px; }
  .statusdot { width: 8px; height: 8px; flex: 0 0 8px; border-radius: 50%; background: #89939e; }
  .statusdot.ready { background: #e09a4f; }
  .statusdot.live { background: #78bd55; }
  .statusdot.error { background: #f06a6a; }
  .canvas-shell { position: relative; min-height: 0; overflow: hidden; border: 1px solid var(--line); border-radius: 4px; background: #000; }
  #canvas { display: block; width: 100%; height: 100%; min-height: 0; background: #000; image-rendering: pixelated; }
  .placeholder { position: absolute; inset: 0; display: grid; place-content: center; gap: 4px; padding: 20px; text-align: center; pointer-events: none; }
  .placeholder span { color: var(--muted); font-size: 12px; }
  @media (max-width: 580px) {
    .topbar { grid-template-columns: 1fr; }
    .notices, .controls { grid-column: 1; }
    .hint { display: none; }
  }
"#;
