pub(super) const SCRIPT: &str = r#"
  (() => {
    const MODES = ["startup", "addressable", "pulse", "scan", "signal", "phosphor", "draw-loop"];
    const stage = document.getElementById("stage");
    const paths = Array.from(document.querySelectorAll(".logo-path"));
    const host = window.LinceWidgetHost || null;
    const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";
    const storageKey = "lince-logo-led-mode/" + instanceId;
    let mode = MODES[0];
    let receivedHostState = false;

    function normalizeMode(value) {
      return MODES.includes(value) ? value : null;
    }

    function readFallbackMode() {
      try { return normalizeMode(localStorage.getItem(storageKey)); }
      catch (_error) { return null; }
    }

    function writeFallbackMode(nextMode) {
      try { localStorage.setItem(storageKey, nextMode); }
      catch (_error) {}
    }

    function applyMode(nextMode, persist = false) {
      mode = normalizeMode(nextMode) || MODES[0];
      stage.dataset.mode = mode;
      stage.setAttribute("aria-label", "Lince logo lighting mode: " + mode + ". Activate to change mode.");
      if (!persist) return;
      writeFallbackMode(mode);
      host?.patchCardState?.({ linceLogoLed: { mode } });
    }

    function modeFromCardState(cardState) {
      return normalizeMode(cardState?.linceLogoLed?.mode ?? cardState?.lince_logo_led?.mode);
    }

    function cycleMode() {
      const currentIndex = MODES.indexOf(mode);
      applyMode(MODES[(currentIndex + 1) % MODES.length], true);
    }

    paths.forEach((path, index) => {
      const length = Math.ceil(path.getTotalLength());
      path.style.setProperty("--path-length", String(length));
      path.style.setProperty("--draw-delay", `${120 + index * 80}ms`);
    });

    applyMode(modeFromCardState(host?.getCardState?.()) || readFallbackMode() || MODES[0]);
    host?.onCardState?.((cardState) => {
      const hostMode = modeFromCardState(cardState);
      if (hostMode) {
        receivedHostState = true;
        applyMode(hostMode);
      } else if (!receivedHostState) {
        applyMode(readFallbackMode() || mode);
      }
    });

    stage.addEventListener("click", cycleMode);
    stage.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      cycleMode();
    });
  })();
"#;
