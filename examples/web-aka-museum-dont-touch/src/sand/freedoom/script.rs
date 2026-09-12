pub(super) const BOOTSTRAP: &str = r#"
  (() => {
    const launchButton = document.getElementById("launch-button");
    const fullscreenButton = document.getElementById("fullscreen-button");
    const reloadButton = document.getElementById("reload-button");
    const statusDot = document.getElementById("status-dot");
    const statusText = document.getElementById("status-text");
    const runtimeLog = document.getElementById("runtime-log");
    const placeholder = document.getElementById("placeholder");
    const canvas = document.getElementById("canvas");
    const autoLaunchRequested = new URLSearchParams(location.search).get("autostart") === "1";
    const startArgs = [
      "-iwad", "doom1.wad", "-window", "-nogui", "-nomusic", "-config", "default.cfg"
    ];
    let runtimeReady = false;
    let pendingLaunch = false;
    let started = false;
    let logLines = ["Booting archive assets..."];

    function setStatus(text, tone = "") {
      statusText.textContent = text;
      statusDot.className = "statusdot" + (tone ? " " + tone : "");
    }

    function writeLog(value) {
      const text = String(value ?? "").trim();
      if (!text) return;
      logLines.push(text);
      logLines = logLines.slice(-18);
      runtimeLog.textContent = logLines.join("\n");
      runtimeLog.scrollTop = runtimeLog.scrollHeight;
    }

    function startGame() {
      if (started) {
        setStatus("Freedoom is already running. Reload for a fresh session.", "live");
        return;
      }
      if (!runtimeReady) {
        pendingLaunch = true;
        setStatus("Still loading the local Freedoom engine...");
        return;
      }
      if (typeof window.callMain !== "function") {
        setStatus("The wasm runtime did not expose callMain().", "error");
        return;
      }

      pendingLaunch = false;
      started = true;
      launchButton.disabled = true;
      launchButton.textContent = "Running";
      fullscreenButton.disabled = false;
      placeholder.hidden = true;
      writeLog("Starting solo Freedoom from local archive assets.");
      setStatus("Freedoom is starting. Click the canvas to capture the pointer.", "live");
      setTimeout(() => canvas.focus(), 50);

      try {
        window.callMain(startArgs);
      } catch (error) {
        started = false;
        launchButton.disabled = false;
        launchButton.textContent = "Retry";
        fullscreenButton.disabled = true;
        writeLog(error?.stack || error);
        setStatus("Freedoom failed to start.", "error");
      }
    }

    launchButton.addEventListener("click", startGame);
    reloadButton.addEventListener("click", () => location.reload());
    fullscreenButton.addEventListener("click", () => {
      if (typeof window.Module?.requestFullscreen === "function") {
        window.Module.requestFullscreen(true, false);
      } else {
        canvas.requestFullscreen?.();
      }
    });
    canvas.addEventListener("contextmenu", (event) => event.preventDefault());
    canvas.addEventListener("webglcontextlost", (event) => {
      event.preventDefault();
      writeLog("WebGL context lost. Reload the session to recover.");
      setStatus("WebGL context lost. Reload the session.", "error");
    });

    window.Module = {
      noInitialRun: true,
      arguments: [],
      elementPointerLock: true,
      locateFile(path) { return path; },
      preRun() {
        window.Module.FS.createPreloadedFile("", "doom1.wad", "doom1.wad", true, true);
        window.Module.FS.createPreloadedFile("", "default.cfg", "default.cfg", true, true);
      },
      onRuntimeInitialized() {
        runtimeReady = true;
        launchButton.disabled = false;
        setStatus("Engine ready. Launch when you want to start.", "ready");
        writeLog("Engine loaded. doom1.wad and default.cfg are ready.");
        if (pendingLaunch || autoLaunchRequested) startGame();
      },
      onExit(status) {
        started = false;
        launchButton.disabled = false;
        launchButton.textContent = "Launch again";
        fullscreenButton.disabled = true;
        writeLog("Freedoom exited with status " + status + ".");
        setStatus("Freedoom exited.", "ready");
      },
      onAbort(reason) {
        started = false;
        launchButton.disabled = false;
        launchButton.textContent = "Retry";
        fullscreenButton.disabled = true;
        const message = String(reason ?? "unknown failure").trim() || "unknown failure";
        writeLog("Abort: " + message);
        setStatus("Freedoom aborted: " + message, "error");
      },
      print: writeLog,
      printErr: writeLog,
      setStatus(text) {
        if (text) setStatus(text, started ? "live" : runtimeReady ? "ready" : "");
      },
      canvas
    };
  })();
"#;
