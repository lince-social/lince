use {
    crate::domain::lince_package::{LincePackage, PackageManifest},
    std::collections::BTreeMap,
};

const ENGINE_JS: &[u8] = include_bytes!("vendor/websockets-doom.js");
const ENGINE_WASM: &[u8] = include_bytes!("vendor/websockets-doom.wasm");
const FREEDOOM_WAD: &[u8] = include_bytes!("vendor/doom1.wad");
const COPYING_TEXT: &str = include_str!("vendor/COPYING.txt");
const CREDITS_TEXT: &str = include_str!("vendor/CREDITS.txt");
const DEFAULT_CFG: &str = r#"mouse_sensitivity 8
show_messages 1
screenblocks 10
detaillevel 0
sfx_volume 8
music_volume 0
use_mouse 1
mouseb_fire 0
mouseb_strafe 1
mouseb_forward 2
key_right 0xae
key_left 0xac
key_up 0xad
key_down 0xaf
use_joystick 0
"#;

pub(crate) fn package() -> LincePackage {
    let manifest = PackageManifest {
        icon: "D".into(),
        title: "Freedoom Portal".into(),
        author: "Lince Labs".into(),
        version: "0.2.0".into(),
        description: "Self-contained Freedoom sand with local wasm, local WAD data, and no remote iframe dependency.".into(),
        details: "Starts a solo Freedoom Phase 1 session from a .lince archive. The package includes the wasm runtime, the WAD asset, default config, and the FreeDM/Freedoom license files.".into(),
        initial_width: 6,
        initial_height: 6,
        requires_server: false,
        permissions: vec![],
    };

    let mut assets = BTreeMap::new();
    assets.insert("websockets-doom.js".into(), ENGINE_JS.to_vec());
    assets.insert("websockets-doom.wasm".into(), ENGINE_WASM.to_vec());
    assets.insert("doom1.wad".into(), FREEDOOM_WAD.to_vec());
    assets.insert("default.cfg".into(), DEFAULT_CFG.as_bytes().to_vec());
    assets.insert("COPYING.txt".into(), COPYING_TEXT.as_bytes().to_vec());
    assets.insert("CREDITS.txt".into(), CREDITS_TEXT.as_bytes().to_vec());

    LincePackage::new_archive(
        Some("freedoom-portal.lince".into()),
        manifest,
        document(),
        "index.html",
        assets,
    )
    .expect("freedoom official sand should render as a valid archive package")
}

fn document() -> String {
    let head = r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Freedoom Portal</title>
    <style>
      :root {
        color-scheme: dark;
        --bg: #060708;
        --panel: rgba(15, 18, 20, 0.96);
        --line: rgba(255, 255, 255, 0.08);
        --line-strong: rgba(255, 255, 255, 0.18);
        --text: #eef3f7;
        --muted: #93a0ad;
        --accent: #f26739;
        --accent-soft: rgba(242, 103, 57, 0.18);
        --shadow: 0 20px 60px rgba(0, 0, 0, 0.45);
      }

      * { box-sizing: border-box; }

      html, body {
        margin: 0;
        height: 100%;
        background:
          radial-gradient(circle at top center, rgba(242, 103, 57, 0.16), transparent 28%),
          linear-gradient(180deg, #060708, #090b0e 55%, #060708);
        color: var(--text);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      body {
        height: 100vh;
        padding: 8px;
      }

      [hidden] {
        display: none !important;
      }

      .app {
        height: calc(100vh - 16px);
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
        gap: 8px;
      }

      .panel {
        border: 1px solid var(--line);
        border-radius: 14px;
        background: linear-gradient(180deg, var(--panel), rgba(10, 12, 15, 0.98));
        box-shadow: none;
      }

      .topbar, .stage {
        padding: 10px;
      }

      .eyebrow {
        color: var(--muted);
        font-size: 0.62rem;
        font-weight: 700;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }

      .title {
        margin: 3px 0 0;
        font-size: 1rem;
        letter-spacing: -0.03em;
      }

      .copy, .hint, .meta, .statusText, .log {
        font-size: 0.76rem;
        line-height: 1.5;
      }

      .copy, .hint, .meta, .statusText, .log {
        color: var(--muted);
      }

      .topbar {
        display: grid;
        gap: 8px;
      }

      .topbarMain {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        flex-wrap: wrap;
        gap: 10px 14px;
      }

      .copy {
        margin: 4px 0 0;
        max-width: 78ch;
        line-height: 1.4;
      }

      .button {
        min-height: 34px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 0 12px;
        border: 1px solid var(--line);
        border-radius: 10px;
        background: rgba(255, 255, 255, 0.03);
        color: inherit;
        text-decoration: none;
        cursor: pointer;
        transition: border-color 120ms ease, background 120ms ease, opacity 120ms ease;
        white-space: nowrap;
      }

      .button:hover:not(:disabled) {
        border-color: var(--line-strong);
        background: rgba(255, 255, 255, 0.06);
      }

      .button:disabled {
        opacity: 0.55;
        cursor: wait;
      }

      .button--primary {
        border-color: rgba(242, 103, 57, 0.34);
        background: var(--accent-soft);
        color: #ffd9cb;
        font-weight: 700;
      }

      .topbarControls {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 8px;
      }

      .toolbarMeta {
        margin-left: auto;
        display: flex;
        align-items: center;
        gap: 8px;
        flex-wrap: wrap;
        justify-content: flex-end;
      }

      .meta {
        padding: 0 2px;
      }

      .linkChip,
      .runtimeSummary {
        min-height: 34px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: 0 12px;
        border: 1px solid var(--line);
        border-radius: 10px;
        background: rgba(255, 255, 255, 0.03);
        color: var(--muted);
        text-decoration: none;
        cursor: pointer;
      }

      .linkChip:hover,
      .runtimeSummary:hover {
        border-color: var(--line-strong);
        background: rgba(255, 255, 255, 0.06);
        color: var(--text);
      }

      .runtimeDetails {
        margin-left: 4px;
      }

      .runtimeDetails[open] {
        flex-basis: 100%;
        margin-left: 0;
      }

      .runtimeSummary {
        list-style: none;
      }

      .runtimeSummary::-webkit-details-marker {
        display: none;
      }

      .stage {
        min-height: 0;
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
        gap: 8px;
      }

      .statusBar {
        display: flex;
        gap: 10px;
        align-items: center;
        justify-content: space-between;
        flex-wrap: wrap;
        padding: 8px 10px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: rgba(255, 255, 255, 0.02);
      }

      .statusDot {
        width: 10px;
        height: 10px;
        border-radius: 999px;
        background: #8f98a3;
        box-shadow: 0 0 0 4px rgba(143, 152, 163, 0.14);
      }

      .statusDot.ready { background: #d78339; box-shadow: 0 0 0 4px rgba(215, 131, 57, 0.16); }
      .statusDot.live { background: #8ccf5f; box-shadow: 0 0 0 4px rgba(140, 207, 95, 0.16); }
      .statusDot.error { background: #ff6b6b; box-shadow: 0 0 0 4px rgba(255, 107, 107, 0.14); }

      .statusMain {
        display: flex;
        align-items: center;
        gap: 10px;
      }

      .canvasWrap {
        min-height: 0;
        height: 100%;
        display: grid;
      }

      .canvasShell {
        position: relative;
        height: 100%;
        min-height: 0;
        display: grid;
        border: 1px solid var(--line);
        border-radius: 12px;
        overflow: hidden;
        background: #000;
      }

      .placeholder {
        position: absolute;
        inset: 0;
        display: grid;
        place-items: center;
        padding: 20px;
        text-align: center;
        background:
          linear-gradient(180deg, rgba(0, 0, 0, 0.16), rgba(0, 0, 0, 0.42)),
          radial-gradient(circle at center, rgba(255, 255, 255, 0.04), transparent 50%);
        pointer-events: none;
      }

      .placeholderTitle {
        font-weight: 700;
        color: var(--text);
        margin-bottom: 6px;
      }

      #canvas {
        display: block;
        width: 100%;
        height: 100%;
        min-height: 0;
        background: #000;
        image-rendering: pixelated;
      }

      .log {
        margin: 8px 0 0;
        max-height: 140px;
        overflow: auto;
        padding: 10px 12px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: rgba(0, 0, 0, 0.22);
        font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
        white-space: pre-wrap;
        font-size: 0.7rem;
      }

      @media (max-width: 760px) {
        .topbarMain {
          flex-direction: column;
          align-items: stretch;
        }

        .toolbarMeta {
          margin-left: 0;
          justify-content: flex-start;
        }
      }
    </style>
  </head>
  <body>
    <div class="app">
      <section class="panel topbar">
        <div class="topbarMain">
          <div>
            <div class="eyebrow">free to use doom</div>
            <h1 class="title">Freedoom Portal</h1>
            <p class="copy">
              Local .lince archive. Wasm engine, Freedoom Phase 1 WAD, default config, and license files are bundled inside the sand.
            </p>
          </div>
          <div class="toolbarMeta">
            <div class="meta">Solo mode</div>
            <div class="meta">WASD move</div>
            <div class="meta">Mouse aim + fire</div>
            <a class="linkChip" href="COPYING.txt" target="_blank" rel="noreferrer">License</a>
            <a class="linkChip" href="CREDITS.txt" target="_blank" rel="noreferrer">Credits</a>
          </div>
        </div>
        <div class="topbarControls">
          <button id="launch-button" class="button button--primary" type="button">Launch</button>
          <button id="fullscreen-button" class="button" type="button" disabled>Fullscreen</button>
          <button id="reload-button" class="button" type="button">Reload</button>
          <details class="runtimeDetails">
            <summary class="runtimeSummary">Runtime log</summary>
            <pre id="runtime-log" class="log" aria-live="polite">Booting archive assets...</pre>
          </details>
        </div>
      </section>

      <section class="panel stage">
        <div class="statusBar">
          <div class="statusMain">
            <div id="status-dot" class="statusDot"></div>
            <div id="status-text" class="statusText">Loading the local Freedoom engine...</div>
          </div>
          <div class="hint">Click inside the canvas once the game starts to lock the pointer.</div>
        </div>

        <div class="canvasWrap">
          <div id="canvas-shell" class="canvasShell">
            <canvas id="canvas" oncontextmenu="event.preventDefault()" tabindex="-1"></canvas>
            <div id="placeholder" class="placeholder">
              <div>
                <div class="placeholderTitle">Freedoom is packaged locally now</div>
                <div class="hint">
                  Launch starts the bundled local wasm build directly from this archive.
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
    <script>
"##;
    let script = bootstrap_script();
    let tail = r##"
    </script>
    <script src="websockets-doom.js"></script>
  </body>
</html>
"##;

    format!("{head}{script}{tail}")
}

fn bootstrap_script() -> &'static str {
    r##"
      (() => {
        window.__LINCE_WIDGET_HOST__ = false;
        const launchButton = document.getElementById("launch-button");
        const fullscreenButton = document.getElementById("fullscreen-button");
        const reloadButton = document.getElementById("reload-button");
        const statusDot = document.getElementById("status-dot");
        const statusText = document.getElementById("status-text");
        const runtimeLog = document.getElementById("runtime-log");
        const placeholder = document.getElementById("placeholder");
        const canvasShell = document.getElementById("canvas-shell");
        const canvas = document.getElementById("canvas");

        const autoLaunchRequested =
          new URLSearchParams(window.location.search).get("autostart") === "1";

        const startArgs = [
          "-iwad", "doom1.wad",
          "-window",
          "-nogui",
          "-nomusic",
          "-config", "default.cfg",
          "-servername", "doomflare",
          "-nodes", "4"
        ];

        let runtimeReady = false;
        let pendingLaunch = false;
        let started = false;
        let logLines = ["Booting archive assets..."];

        function setStatus(text, tone) {
          statusText.textContent = text;
          statusDot.className = "statusDot" + (tone ? " " + tone : "");
        }

        function writeLog(value) {
          const text = String(value ?? "").trim();
          if (!text) {
            return;
          }
          logLines.push(text);
          if (logLines.length > 18) {
            logLines = logLines.slice(logLines.length - 18);
          }
          runtimeLog.textContent = logLines.join("\n");
          runtimeLog.scrollTop = runtimeLog.scrollHeight;
        }

        function showCanvas() {
          placeholder.hidden = true;
        }

        function startGame() {
          if (started) {
            setStatus("Freedoom is already running. Use Reload Session to boot a fresh copy.", "live");
            return;
          }

          if (!runtimeReady) {
            pendingLaunch = true;
            setStatus("Still loading the local Freedoom engine...", "");
            return;
          }

          if (typeof window.callMain !== "function") {
            setStatus("The wasm runtime loaded, but callMain() is not available. Reload and try again.", "error");
            return;
          }

          pendingLaunch = false;
          started = true;
          launchButton.disabled = true;
          launchButton.textContent = "Running";
          fullscreenButton.disabled = false;
          showCanvas();
          writeLog("Starting solo Freedoom from local archive assets.");
          setStatus("Freedoom is starting. Click inside the canvas to capture the pointer.", "live");

          window.setTimeout(() => {
            canvas.focus();
          }, 50);

          try {
            window.callMain(startArgs);
          } catch (error) {
            started = false;
            launchButton.disabled = false;
            launchButton.textContent = "Retry Launch";
            fullscreenButton.disabled = true;
            writeLog(error && error.stack ? error.stack : error);
            setStatus("Freedoom failed to start.", "error");
          }
        }

        launchButton.addEventListener("click", startGame);
        reloadButton.addEventListener("click", () => window.location.reload());
        fullscreenButton.addEventListener("click", () => {
          if (window.Module && typeof window.Module.requestFullscreen === "function") {
            window.Module.requestFullscreen(true, false);
          } else if (canvas.requestFullscreen) {
            canvas.requestFullscreen();
          }
        });

        canvas.addEventListener("webglcontextlost", (event) => {
          event.preventDefault();
          writeLog("WebGL context lost. Reload the session to recover.");
          setStatus("WebGL context lost. Reload the session.", "error");
        });

        setStatus("Loading the local Freedoom engine...", "");

        window.Module = {
          noInitialRun: true,
          arguments: [],
          elementPointerLock: true,
          locateFile(path) {
            return path;
          },
          preRun() {
            window.Module.FS.createPreloadedFile("", "doom1.wad", "doom1.wad", true, true);
            window.Module.FS.createPreloadedFile("", "default.cfg", "default.cfg", true, true);
          },
          onRuntimeInitialized() {
            runtimeReady = true;
            launchButton.disabled = false;
            setStatus("Engine ready. Launch Freedoom when you want to start.", "ready");
            writeLog("Engine loaded. doom1.wad and default.cfg are ready.");
            if (pendingLaunch || autoLaunchRequested) {
              startGame();
            }
          },
          onExit(status) {
            started = false;
            launchButton.disabled = false;
            launchButton.textContent = "Launch Again";
            fullscreenButton.disabled = true;
            writeLog("Freedoom exited with status " + status + ".");
            setStatus("Freedoom exited. You can launch it again or reload the session.", "ready");
          },
          onAbort(reason) {
            started = false;
            launchButton.disabled = false;
            launchButton.textContent = "Retry Launch";
            fullscreenButton.disabled = true;
            const reasonText = String(reason ?? "unknown failure").trim() || "unknown failure";
            writeLog("Abort: " + reasonText);
            setStatus("Freedoom aborted: " + reasonText, "error");
          },
          print(text) {
            writeLog(text);
          },
          printErr(text) {
            writeLog(text);
          },
          setStatus(text) {
            if (!text) {
              return;
            }
            setStatus(text, started ? "live" : runtimeReady ? "ready" : "");
          },
          canvas
        };
      })();
    "##
}
