use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"local-terminal.html"#,
        lang: r#"pt-BR"#,
        manifest: PackageManifest {
            icon: r#"⌘"#.into(),
            title: r#"Local Terminal"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.3.0"#.into(),
            description: r#"Shell local minimalista rodando pelo backend do host."#.into(),
            details: r#"Abre uma sessao de shell persistente no backend local do host usando as rotas /host/terminal/sessions."#.into(),
            initial_width: 5,
            initial_height: 4,
            permissions: vec![r#"terminal_session"#.into()],
        },
        head_links: vec![crate::sand::HeadLink { rel: r#"stylesheet"#, href: r#"https://cdn.jsdelivr.net/npm/xterm@5.3.0/css/xterm.min.css"# }],
        inline_styles: vec![r#"      :root {
        color-scheme: dark;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        --bg: #0b0d11;
        --line: rgba(255, 255, 255, 0.08);
        --line-strong: rgba(255, 255, 255, 0.14);
        --text: #e8edf5;
        --muted: #8c94a1;
        --danger: #ff8b9a;
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        margin: 0;
        min-height: 100%;
        background: transparent;
      }

      body {
        min-height: 100vh;
        overflow: hidden;
        color: var(--text);
      }

      .app {
        position: relative;
        min-height: 100vh;
        background: var(--bg);
      }

      .app::before {
        content: "";
        position: absolute;
        inset: 0;
        border: 1px solid rgba(255, 255, 255, 0.03);
        pointer-events: none;
      }

      .floating-actions {
        position: absolute;
        top: 12px;
        right: 12px;
        z-index: 3;
        display: flex;
        gap: 8px;
      }

      .floating-button,
      .status-chip {
        backdrop-filter: blur(18px);
      }

      .floating-button {
        display: inline-grid;
        place-items: center;
        width: 34px;
        height: 34px;
        padding: 0;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: rgba(14, 16, 20, 0.88);
        color: var(--text);
        cursor: pointer;
        transition:
          border-color 160ms ease,
          background 160ms ease,
          color 160ms ease,
          transform 160ms ease;
      }

      .floating-button:hover {
        border-color: var(--line-strong);
        background: rgba(20, 23, 28, 0.94);
        transform: translateY(-1px);
      }

      .floating-button--danger {
        color: var(--danger);
      }

      .floating-button svg {
        width: 15px;
        height: 15px;
      }

      .status-chip {
        position: absolute;
        left: 12px;
        bottom: 12px;
        z-index: 3;
        display: inline-flex;
        align-items: center;
        gap: 8px;
        min-height: 28px;
        padding: 0 10px;
        border: 1px solid var(--line);
        border-radius: 999px;
        background: rgba(14, 16, 20, 0.82);
        color: var(--muted);
        font-size: 10px;
        letter-spacing: 0.08em;
        text-transform: uppercase;
      }

      .status-chip::before {
        content: "";
        width: 7px;
        height: 7px;
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.3);
      }

      .status-chip[data-state="ready"]::before {
        background: #87efb3;
        box-shadow: 0 0 0 5px rgba(135, 239, 179, 0.08);
      }

      .status-chip[data-state="error"]::before {
        background: #ff8b9a;
        box-shadow: 0 0 0 5px rgba(255, 139, 154, 0.08);
      }

      .shell {
        position: absolute;
        inset: 0;
        padding: 12px 12px 46px;
        overflow: hidden;
        background: var(--bg);
      }

      #terminal {
        width: 100%;
        height: 100%;
      }

      .xterm {
        height: 100%;
        padding: 0;
      }

      .xterm-viewport {
        overflow-y: auto !important;
        scrollbar-width: thin;
      }
    "#],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::src(r#"https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.min.js"#),
            crate::sand::WidgetScript::inline(r##"      const statusEl = document.getElementById("status");
      const terminalRoot = document.getElementById("terminal");
      const shellSurface = document.getElementById("shell-surface");
      const restartButton = document.getElementById("restart-button");
      const interruptButton = document.getElementById("interrupt-button");

      let terminal = null;
      let sessionId = null;
      let cursor = 0;
      let pollTimer = null;
      let sending = false;
      let pendingInput = [];

      function setStatus(text, tone = "busy") {
        statusEl.textContent = text;
        statusEl.dataset.state = tone;
      }

      function ensureTerminal() {
        if (terminal) {
          return terminal;
        }

        terminal = new Terminal({
          cursorBlink: true,
          fontFamily: '"SFMono-Regular", "IBM Plex Mono", monospace',
          fontSize: 12.5,
          lineHeight: 1.28,
          scrollback: 3000,
          allowTransparency: false,
          theme: {
            background: "#0b0d11",
            foreground: "#e8edf5",
            cursor: "#f3f6fb",
            cursorAccent: "#0b0d11",
            selectionBackground: "rgba(255, 255, 255, 0.16)",
            black: "#0b0d11",
            red: "#ff8b9a",
            green: "#9ee6b7",
            yellow: "#f1d38a",
            blue: "#9ebcff",
            magenta: "#d4b4ff",
            cyan: "#8dd8ef",
            white: "#e8edf5",
            brightBlack: "#7f8896",
            brightRed: "#ffb0bb",
            brightGreen: "#baf2cc",
            brightYellow: "#f7e4aa",
            brightBlue: "#bfd1ff",
            brightMagenta: "#e0cbff",
            brightCyan: "#b7e9f7",
            brightWhite: "#ffffff"
          }
        });

        terminal.open(terminalRoot);
        terminal.focus();
        terminal.onData((data) => {
          queueInput(data);
        });
        window.addEventListener("resize", fitTerminal);
        window.setTimeout(fitTerminal, 0);
        return terminal;
      }

      function fitTerminal() {
        if (!terminalRoot || !shellSurface || !terminal) {
          return;
        }

        terminalRoot.style.height = shellSurface.clientHeight + "px";
        terminal.scrollToBottom();
      }

      async function createSession() {
        const response = await fetch("/host/terminal/sessions", {
          method: "POST"
        });
        const payload = await response.json().catch(() => null);
        if (!response.ok) {
          throw new Error(payload?.error || "Falha ao abrir a shell local.");
        }

        sessionId = payload.id;
        cursor = payload.nextCursor || 0;
        setStatus("Online", "ready");
      }

      async function destroySession() {
        if (!sessionId) {
          return;
        }

        const currentId = sessionId;
        sessionId = null;
        window.clearTimeout(pollTimer);
        pollTimer = null;

        await fetch("/host/terminal/sessions/" + currentId, {
          method: "DELETE"
        }).catch(() => null);
      }

      function queueInput(data) {
        if (!sessionId || !data) {
          return;
        }

        pendingInput.push(data);
        void flushInput();
      }

      async function flushInput() {
        if (sending || !sessionId || pendingInput.length === 0) {
          return;
        }

        sending = true;
        try {
          while (pendingInput.length > 0 && sessionId) {
            const input = pendingInput.shift();
            const response = await fetch("/host/terminal/sessions/" + sessionId + "/input", {
              method: "POST",
              headers: {
                "Content-Type": "application/json"
              },
              body: JSON.stringify({ input })
            });

            if (!response.ok) {
              const payload = await response.json().catch(() => null);
              throw new Error(payload?.error || "Falha ao escrever no terminal.");
            }
          }
        } catch (error) {
          setStatus("Erro", "error");
        } finally {
          sending = false;
        }
      }

      async function pollOutput() {
        if (!sessionId) {
          return;
        }

        try {
          const response = await fetch(
            "/host/terminal/sessions/" + sessionId + "/output?cursor=" + encodeURIComponent(cursor)
          );
          const payload = await response.json().catch(() => null);
          if (!response.ok) {
            throw new Error(payload?.error || "Falha ao ler o output do terminal.");
          }

          if (payload.truncated) {
            ensureTerminal().reset();
          }

          if (payload.data) {
            ensureTerminal().write(payload.data);
          }

          cursor = payload.session?.nextCursor || cursor;

          if (payload.session?.closed) {
            const code = payload.session?.exitCode;
            setStatus(code == null ? "Encerrado" : "Exit " + code, "error");
            sessionId = null;
            return;
          }
        } catch (error) {
          setStatus("Erro", "error");
        }

        pollTimer = window.setTimeout(pollOutput, 120);
      }

      async function restartSession() {
        window.clearTimeout(pollTimer);
        pollTimer = null;
        pendingInput = [];
        sending = false;

        if (terminal) {
          terminal.reset();
          terminal.clear();
        }

        await destroySession();
        await start();
      }

      function sendInterrupt() {
        if (!sessionId) {
          return;
        }

        queueInput("\u0003");
      }

      async function start() {
        ensureTerminal();
        setStatus("Abrindo", "busy");
        await createSession();
        pollTimer = window.setTimeout(pollOutput, 40);
        terminal.focus();
      }

      restartButton.addEventListener("click", () => {
        void restartSession();
      });

      interruptButton.addEventListener("click", () => {
        sendInterrupt();
      });

      shellSurface.addEventListener("pointerdown", () => {
        terminal?.focus();
      });

      window.addEventListener("beforeunload", () => {
        void destroySession();
      });

      void start().catch((error) => {
        ensureTerminal().writeln("");
        ensureTerminal().writeln("Falha ao abrir o terminal.");
        setStatus("Erro", "error");
        console.error(error);
      });
    "##)],
    }
}

fn body() -> Markup {
    let floating_buttons = [
        (
            "restart-button",
            "floating-button",
            "Reiniciar sessao",
            ["M20 5v6h-6", "M20 11a8 8 0 1 0 2 5.5"],
        ),
        (
            "interrupt-button",
            "floating-button floating-button--danger",
            "Enviar Ctrl+C",
            ["M6 6l12 12", "M18 6 6 18"],
        ),
    ];

    html! {
        main class="app" {
            div class="floating-actions" {
                @for (id, class_name, aria_label, paths) in floating_buttons {
                    button class=(class_name) id=(id) type="button" aria-label=(aria_label) {
                        svg
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="1.7"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            aria-hidden="true"
                        {
                            @for path_value in paths {
                                path d=(path_value) {}
                            }
                        }
                    }
                }
            }
            div class="shell" id="shell-surface" {
                div id="terminal" {}
            }
            div class="status-chip" id="status" data-state="busy" { "Abrindo" }
        }
    }
}
