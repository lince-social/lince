pub(super) const CSS: &str = r#"
  :root { color-scheme: dark; --line-width: 7; --stroke: #f8fafc; }
  * { box-sizing: border-box; -webkit-tap-highlight-color: transparent; }
  html, body { margin: 0; min-height: 100%; background: transparent; }
  body { min-height: 100vh; overflow: hidden; }
  .stage { display: grid; place-items: center; width: 100%; min-height: 100vh; outline: 0; cursor: pointer; }
  .stage:focus-visible { outline: 1px solid #67e8f9; outline-offset: -2px; }
  .lockup { display: grid; justify-items: center; gap: 12px; width: min(80vmin, 420px); }
  svg { width: 100%; height: auto; overflow: visible; }
  .logo-path {
    fill: none; stroke: var(--stroke); stroke-width: var(--line-width); stroke-linecap: round;
    stroke-linejoin: round; vector-effect: non-scaling-stroke;
  }
  .wordmark {
    color: #f8fafc; font: 600 14px/1 system-ui, -apple-system, sans-serif;
    letter-spacing: .34em; text-transform: uppercase; user-select: none;
  }
  .stage[data-mode="startup"] .logo-path {
    stroke-dasharray: var(--path-length, 1200); stroke-dashoffset: var(--path-length, 1200);
    animation: draw-on 1.7s cubic-bezier(.33, 1, .68, 1) forwards, monochrome 5.4s ease-in-out infinite 1.7s;
    animation-delay: var(--draw-delay, 0ms), 1.7s;
  }
  .stage[data-mode="addressable"] .logo-path { stroke: url(#ledGradient); animation: flicker 1.8s linear infinite, led-glow 2.4s ease-in-out infinite alternate; }
  .stage[data-mode="pulse"] .logo-path { animation: pulse 2.6s ease-in-out infinite, lime-cyan 6s linear infinite; }
  .stage[data-mode="scan"] .logo-path { animation: flicker 2.1s linear infinite, cool 7.4s linear infinite; }
  .stage[data-mode="signal"] .logo-path { animation: signal 1.15s steps(2, end) infinite, magenta-cyan 5.2s linear infinite; }
  .stage[data-mode="phosphor"] .logo-path { animation: phosphor 3.8s ease-in-out infinite, acid 8.5s linear infinite; }
  .stage[data-mode="draw-loop"] .logo-path {
    stroke: #f8fafc; stroke-dasharray: var(--path-length, 1200); stroke-dashoffset: var(--path-length, 1200);
    animation: draw-loop 6.2s ease-in-out infinite;
  }
  @keyframes draw-on { to { stroke-dashoffset: 0; } }
  @keyframes monochrome {
    0%, 100% { opacity: .82; filter: drop-shadow(0 0 6px rgb(255 255 255 / .12)); }
    50% { opacity: 1; filter: drop-shadow(0 0 16px rgb(255 255 255 / .2)); }
  }
  @keyframes pulse {
    0%, 100% { opacity: .72; filter: drop-shadow(0 0 8px rgb(125 211 252 / .12)); }
    50% { opacity: 1; filter: drop-shadow(0 0 22px rgb(163 230 53 / .22)); }
  }
  @keyframes lime-cyan { 0%, 100% { stroke: #7dd3fc; } 50% { stroke: #bef264; } }
  @keyframes flicker { 0%, 100%, 12%, 58% { opacity: 1; } 8%, 54% { opacity: .78; } }
  @keyframes cool { 0%, 100% { stroke: #f8fafc; } 50% { stroke: #67e8f9; } }
  @keyframes signal { 0%, 100% { opacity: 1; } 50% { opacity: .32; } }
  @keyframes magenta-cyan { 0%, 100% { stroke: #f472b6; } 50% { stroke: #22d3ee; } }
  @keyframes phosphor {
    0%, 100% { opacity: .86; filter: drop-shadow(0 0 9px rgb(74 222 128 / .14)); }
    50% { opacity: 1; filter: drop-shadow(0 0 22px rgb(190 242 100 / .2)); }
  }
  @keyframes acid { 0%, 100% { stroke: #86efac; } 50% { stroke: #bef264; } }
  @keyframes led-glow {
    from { filter: drop-shadow(0 0 8px rgb(103 232 249 / .14)); }
    to { filter: drop-shadow(0 0 24px rgb(244 114 182 / .22)); }
  }
  @keyframes draw-loop {
    0%, 100% { stroke-dashoffset: var(--path-length, 1200); opacity: .12; }
    20%, 68% { stroke-dashoffset: 0; opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .logo-path { animation-duration: .01ms !important; animation-iteration-count: 1 !important; }
  }
"#;
