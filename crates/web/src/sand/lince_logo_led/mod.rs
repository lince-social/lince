use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) const FEATURE_FLAG: &str = "sand.lince_logo_led";

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"lince-logo-led.html"#,
        lang: r#"pt-BR"#,
        manifest: PackageManifest {
            icon: r#"◈"#.into(),
            title: r#"Lince Logo LED"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.1.0"#.into(),
            description: r#"Logo SVG da Lince com modos visuais animados que trocam a cada clique."#.into(),
            details: r#"Card visual minimalista com a marca da Lince em traço fino, transparente e com modos de luz, fita LED e desenho em loop."#.into(),
            initial_width: 4,
            initial_height: 4,
            requires_server: false,
            permissions: vec![],
        },
        head_links: vec![],
        inline_styles: vec![r#"      :root {
        color-scheme: dark;
        --bg: #000;
        --line-width: 7;
        --stroke: #f8fafc;
        --glow: rgba(255, 255, 255, 0.14);
      }

      * {
        box-sizing: border-box;
        -webkit-tap-highlight-color: transparent;
      }

      html,
      body {
        margin: 0;
        min-height: 100%;
        background: transparent;
      }

      body {
        min-height: 100vh;
        cursor: pointer;
        overflow: hidden;
      }

      .stage {
        display: grid;
        place-items: center;
        width: 100%;
        min-height: 100vh;
        background: transparent;
      }

      .lockup {
        display: grid;
        justify-items: center;
        gap: 12px;
        width: min(80vmin, 420px);
      }

      svg {
        width: 100%;
        height: auto;
        overflow: visible;
      }

      .logo-path {
        fill: none;
        stroke: var(--stroke);
        stroke-width: var(--line-width);
        stroke-linecap: round;
        stroke-linejoin: round;
        vector-effect: non-scaling-stroke;
      }

      .wordmark {
        color: #f8fafc;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        font-size: 0.9rem;
        font-weight: 600;
        letter-spacing: 0.34em;
        text-transform: uppercase;
        user-select: none;
      }

      .stage[data-mode="startup"] .logo-path {
        stroke-dasharray: var(--path-length, 1200);
        stroke-dashoffset: var(--path-length, 1200);
        animation:
          draw-on 1700ms cubic-bezier(0.33, 1, 0.68, 1) forwards,
          monochrome-breathe 5.4s ease-in-out infinite 1.7s;
        animation-delay: var(--draw-delay, 0ms), 1.7s;
      }

      .stage[data-mode="rgb-fade"] .logo-path {
        animation:
          rgb-stroke 6.2s linear infinite,
          soft-fade 2.8s ease-in-out infinite alternate;
      }

      .stage[data-mode="pulse"] .logo-path {
        animation:
          pulse-glow 2.6s ease-in-out infinite,
          lime-cyan-shift 6s linear infinite;
      }

      .stage[data-mode="scan"] .logo-path {
        animation:
          scanline-flicker 2.1s linear infinite,
          cool-shift 7.4s linear infinite;
      }

      .stage[data-mode="signal"] .logo-path {
        animation:
          signal-pop 1.15s steps(2, end) infinite,
          magenta-cyan-shift 5.2s linear infinite;
      }

      .stage[data-mode="phosphor"] .logo-path {
        animation:
          phosphor-glow 3.8s ease-in-out infinite,
          acid-shift 8.5s linear infinite;
      }

      .stage[data-mode="addressable"] .logo-path {
        stroke: url(#ledGradient);
        animation:
          led-flicker 1.8s linear infinite,
          led-glow 2.4s ease-in-out infinite alternate;
      }

      .stage[data-mode="draw-loop"] .logo-path {
        stroke: #f8fafc;
        stroke-dasharray: var(--path-length, 1200);
        stroke-dashoffset: var(--path-length, 1200);
        animation: draw-loop 6.2s ease-in-out infinite;
      }

      @keyframes draw-on {
        to {
          stroke-dashoffset: 0;
        }
      }

      @keyframes monochrome-breathe {
        0%, 100% {
          stroke: #f8fafc;
          opacity: 0.82;
          filter:
            drop-shadow(0 0 4px rgba(255, 255, 255, 0.1))
            drop-shadow(0 0 14px rgba(255, 255, 255, 0.08));
        }
        50% {
          stroke: #ffffff;
          opacity: 1;
          filter:
            drop-shadow(0 0 8px rgba(255, 255, 255, 0.18))
            drop-shadow(0 0 22px rgba(255, 255, 255, 0.12));
        }
      }

      @keyframes rgb-stroke {
        0% {
          stroke: #7dd3fc;
          filter: drop-shadow(0 0 10px rgba(125, 211, 252, 0.18));
        }
        33% {
          stroke: #f472b6;
          filter: drop-shadow(0 0 12px rgba(244, 114, 182, 0.18));
        }
        66% {
          stroke: #a3e635;
          filter: drop-shadow(0 0 12px rgba(163, 230, 53, 0.16));
        }
        100% {
          stroke: #7dd3fc;
          filter: drop-shadow(0 0 10px rgba(125, 211, 252, 0.18));
        }
      }

      @keyframes soft-fade {
        from {
          opacity: 0.68;
        }
        to {
          opacity: 1;
        }
      }

      @keyframes pulse-glow {
        0%, 100% {
          opacity: 0.72;
          filter:
            drop-shadow(0 0 8px rgba(125, 211, 252, 0.12))
            drop-shadow(0 0 18px rgba(163, 230, 53, 0.08));
        }
        50% {
          opacity: 1;
          filter:
            drop-shadow(0 0 14px rgba(125, 211, 252, 0.24))
            drop-shadow(0 0 28px rgba(163, 230, 53, 0.16));
        }
      }

      @keyframes lime-cyan-shift {
        0%, 100% {
          stroke: #7dd3fc;
        }
        50% {
          stroke: #bef264;
        }
      }

      @keyframes scanline-flicker {
        0%, 100% {
          opacity: 1;
        }
        8% {
          opacity: 0.82;
        }
        12% {
          opacity: 1;
        }
        48% {
          opacity: 0.74;
        }
        52% {
          opacity: 1;
        }
      }

      @keyframes cool-shift {
        0%, 100% {
          stroke: #f8fafc;
          filter: drop-shadow(0 0 10px rgba(248, 250, 252, 0.1));
        }
        50% {
          stroke: #67e8f9;
          filter: drop-shadow(0 0 16px rgba(103, 232, 249, 0.16));
        }
      }

      @keyframes signal-pop {
        0%, 100% {
          opacity: 1;
        }
        50% {
          opacity: 0.32;
        }
      }

      @keyframes magenta-cyan-shift {
        0% {
          stroke: #f472b6;
          filter: drop-shadow(0 0 14px rgba(244, 114, 182, 0.18));
        }
        50% {
          stroke: #22d3ee;
          filter: drop-shadow(0 0 14px rgba(34, 211, 238, 0.18));
        }
        100% {
          stroke: #f472b6;
          filter: drop-shadow(0 0 14px rgba(244, 114, 182, 0.18));
        }
      }

      @keyframes phosphor-glow {
        0%, 100% {
          opacity: 0.86;
          filter:
            drop-shadow(0 0 10px rgba(74, 222, 128, 0.12))
            drop-shadow(0 0 22px rgba(190, 242, 100, 0.08));
        }
        50% {
          opacity: 1;
          filter:
            drop-shadow(0 0 18px rgba(74, 222, 128, 0.18))
            drop-shadow(0 0 30px rgba(190, 242, 100, 0.14));
        }
      }

      @keyframes acid-shift {
        0%, 100% {
          stroke: #86efac;
        }
        50% {
          stroke: #bef264;
        }
      }

      @keyframes led-flicker {
        0%, 100% {
          opacity: 1;
        }
        8% {
          opacity: 0.9;
        }
        11% {
          opacity: 1;
        }
        54% {
          opacity: 0.82;
        }
        58% {
          opacity: 1;
        }
      }

      @keyframes led-glow {
        from {
          filter:
            drop-shadow(0 0 8px rgba(255, 255, 255, 0.1))
            drop-shadow(0 0 18px rgba(103, 232, 249, 0.12));
        }
        to {
          filter:
            drop-shadow(0 0 12px rgba(255, 255, 255, 0.12))
            drop-shadow(0 0 26px rgba(244, 114, 182, 0.18));
        }
      }

      @keyframes draw-loop {
        0% {
          stroke-dashoffset: var(--path-length, 1200);
          opacity: 0.12;
        }
        20% {
          stroke-dashoffset: 0;
          opacity: 1;
        }
        68% {
          stroke-dashoffset: 0;
          opacity: 1;
        }
        100% {
          stroke-dashoffset: var(--path-length, 1200);
          opacity: 0.12;
        }
      }
    "#],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(r#"      const MODES = ["startup", "addressable", "pulse", "scan", "signal", "phosphor", "draw-loop"];
      const stage = document.getElementById("stage");
      const paths = Array.from(document.querySelectorAll(".logo-path"));
      const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";
      const storageKey = "lince-logo-led-mode/" + instanceId;

      function readStoredMode() {
        try {
          const value = window.localStorage.getItem(storageKey);
          return MODES.includes(value) ? value : null;
        } catch (error) {
          return null;
        }
      }

      function storeMode(mode) {
        try {
          window.localStorage.setItem(storageKey, mode);
        } catch (error) {
          return;
        }
      }

      function primePathLengths() {
        paths.forEach((path, index) => {
          const length = Math.ceil(path.getTotalLength());
          path.style.setProperty("--path-length", String(length));
          path.style.setProperty("--draw-delay", `${120 + index * 80}ms`);
        });
      }

      function setMode(mode) {
        const nextMode = MODES.includes(mode) ? mode : MODES[0];
        stage.dataset.mode = nextMode;
        storeMode(nextMode);
      }

      function cycleMode() {
        const currentMode = stage.dataset.mode || MODES[0];
        const currentIndex = MODES.indexOf(currentMode);
        const nextMode = MODES[(currentIndex + 1) % MODES.length];
        setMode(nextMode);
      }

      primePathLengths();
      setMode(readStoredMode() || MODES[0]);

      stage.addEventListener("click", () => {
        cycleMode();
      });
    "#)],
    }
}

fn body() -> Markup {
    let gradient_stops = [
        ("0%", "#22d3ee"),
        ("14%", "#22d3ee"),
        ("14%", "#f472b6"),
        ("28%", "#f472b6"),
        ("28%", "#facc15"),
        ("42%", "#facc15"),
        ("42%", "#4ade80"),
        ("56%", "#4ade80"),
        ("56%", "#a78bfa"),
        ("70%", "#a78bfa"),
        ("70%", "#22d3ee"),
        ("100%", "#22d3ee"),
    ];
    let logo_paths = [
        "m444.5 348c-82-82.5-212-133-251.5-155.5-39.5-22.5-51.44-47.5-55.5-70.5-4.06-23 21.5-38.5 34.5-14.5 13 24-1.5 44-9.5 76-8 32-6.6 50.6 4.5 109 11.1 58.4 44.5 248.5-3.5 261.5-26.06 7.06-43-25-20-43.5 23-18.5 76-5.5 73 93.5m213-188c16.5-30.5 29-70.6 85-105 23.71-14.56 55.3-23.85 87.11-29.57 73.56-13.23 159.88-6.65 205.39-65.93 38-49.5 82.88-102.18 112.5-113 19.33-7.06 61-4 69-21.5 8-17.5-15.08-33.24-34.5-20-19.42 13.24-39.1 54.1-11.5 196.5 34.5 178-32.5 178-34.5 267.5m-516.5-63c-47.5 40.5-99.33 19.33-115.5 0-40.5-43-42-125.5-47-156-4-24.4-12.33-47.83-16-56.5 38 20.5 108.5 53 157 87 48.5 34 33.5 92 42 120.5 8.5 28.5 26.66 42.45 59.5 64 16 10.5 41.51 32.37 45.5 68.5 3.5 17 3.26 59.47-12.5 72-15.76 12.53-50.4 23.5-96 19.5-57-5-67-62.5-65.5-77",
        "m908.88 552.5c2 47.17 4.8 151.9 0 193.5-6 52-9.94 79.36-36.38 124.5-27.12 46.3-40 36.5-93.5 106.5-22.54 29.49-46.2 40.86-88.03 46.5-61.47 8.28-75.5-20.96-75.5-31 0-17 0-30 29.53-53.5 29.53-23.5 43.47-60.6-46-63-78.1-2.1-86 36.5-43 63 34.4 21.2 33.56 48.45 31.88 61.5-2.24 17.46-27.07 28.04-69.38 27.5-30.46 0.16-91.9-5.5-139.5-27.5",
        "m504 663.5c22.33 18.33 72.1 76.4 64.5 190m125.54-178c-18.69 23-60.24 78.2-55.04 181",
        "m402.5 890.5c-0.8-43.6-36.14-66.47-52.5-75-108.71-56.67-121.34-70.64-131-103-7.21-24.13-2.7-77.6-1.5-86 1.5 70.5 68.5 134.5 195 130 49.97-1.78 53-45 54.5-63.5 2.21-27.25-0.6-107.9-141-89.5m151.5 304c-35.5-4.67-144.2 4.2-295 77m305.5-31c-53.67 11-171.5 40.3-213.5 69.5m130.5-95l1 15v13m296.5-43c56.67 2.17 189.7 12.5 268.5 36.5m-275 0c41.67 7.67 139.6 29.9 198 57.5",
        "m474.5 374c20.5 15.5 31.5 7.5 62 36.5m28 26c3 4 18.6 21.3 19 70.5-0.5 6.67-1.5 20 4.5 28",
        "m712.5 375.5c-9 5-39.36 28.56-47 68-1.84 9.51-0.71 21.5-4.5 43.5-3.79 22-15.5 36.5-22 47.5",
        "m500.5 494c21.33-39.5 82-123.4 154-143 21.5-6.17 74.87-28.47 119.5-56.5 44.63-28.03 105.82-105.67 130.5-142.5 0 0 2.13 15.2 2.96 25 0.81 9.78 1.63 22.37 2.39 36 3.9 69.48 16.82 147.47-22.35 205-16 23.5-42 58-79.5 76-37.5 18-102 19.5-129 75-10.95 22.5-14.5 72 0 87 14.5 15 38.5 24.5 97.5 28.5 68.85 4.67 94.5-34.94 89.5-75.22",
        "m686 881c26-40.5 74.5-32 130.5-59 44.8-21.6 62.33-62.67 65.5-80.5-1.5 13.33-18 38.5-72 32.5-67.5-7.5-67.92-47.17-68-85.5-0.08-38.33 10-63 52-73 42-10 80.5-7.5 94.5-5",
        "m730 364c8.66-6.12 27.9-17.02 52.14-17.06 36.22-0.07 70.51 34.67 103.86 20.56",
        "m183.75 664.5c0 24-2.82 82.2 0 122.5 5 71.5 64.25 112.67 91.75 139",
        "m305.5 954c0 0 8.96 8.3 15.5 12.5 6.54 4.2 16 9 16 9",
    ];

    html! {
        main id="stage" class="stage" data-mode="startup" aria-label="Lince logo animated card" {
            div class="lockup" {
                svg viewBox="0 0 1105 1105" aria-hidden="true" {
                    defs {
                        linearGradient id="ledGradient" x1="0" y1="0" x2="1105" y2="0" gradientUnits="userSpaceOnUse" {
                            @for (offset, color) in gradient_stops {
                                stop offset=(offset) stop-color=(color) {}
                            }
                            animateTransform
                                attributeName="gradientTransform"
                                type="translate"
                                values="-240 0; 0 0; 240 0; -240 0"
                                dur="4.2s"
                                repeatCount="indefinite" {}
                        }
                    }
                    g {
                        @for path_value in logo_paths {
                            path class="logo-path" d=(path_value) {}
                        }
                    }
                }
                div class="wordmark" { "lince" }
            }
        }
    }
}
