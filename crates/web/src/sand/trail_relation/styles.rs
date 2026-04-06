pub(crate) const INLINE_STYLES: &[&str] = &[r#"
    :root {
        color-scheme: light;
        --bg: #f5f1e8;
        --panel: rgba(255, 252, 247, 0.86);
        --ink: #1f1a17;
        --muted: #6e645b;
        --line: rgba(72, 55, 38, 0.16);
        --accent: #1e6f5c;
        --accent-strong: #0f4f41;
        --warn: #7a3b27;
        --shadow: 0 18px 40px rgba(39, 29, 16, 0.12);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    }

    * { box-sizing: border-box; }

    html, body {
        margin: 0;
        min-height: 100%;
        background:
            radial-gradient(circle at top left, rgba(30, 111, 92, 0.14), transparent 32%),
            linear-gradient(180deg, #fff9f0 0%, var(--bg) 100%);
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
    }

    .trailHeader {
        display: flex;
        justify-content: space-between;
        gap: 12px;
        align-items: flex-start;
    }

    h1, h2 { margin: 0 0 8px; }
    h1 { font-size: 1.35rem; }
    h2 { font-size: 1rem; }

    .small, .status, .warning { margin: 0; color: var(--muted); }
    .warning { color: var(--warn); }

    .grid {
        display: grid;
        gap: 12px;
        grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    }

    .field { display: grid; gap: 6px; font-size: 0.92rem; }

    .fieldInput,
    .button {
        min-height: 38px;
        border-radius: 10px;
        border: 1px solid var(--line);
        background: rgba(255,255,255,0.88);
        color: var(--ink);
        padding: 9px 11px;
    }

    .button {
        cursor: pointer;
        font-weight: 600;
    }

    .buttonAccent {
        background: var(--accent);
        color: #f7fffc;
        border-color: var(--accent-strong);
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
        background: rgba(255,255,255,0.7);
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
    }

    .pill {
        border: 1px solid var(--line);
        border-radius: 999px;
        padding: 3px 8px;
        font-size: 0.82rem;
        color: var(--muted);
        background: rgba(255,255,255,0.66);
    }

    .trailNode {
        border-left: 4px solid rgba(30, 111, 92, 0.2);
    }
"#];
