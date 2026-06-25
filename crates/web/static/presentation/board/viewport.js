import "../../vendored/panzoom.min.js";

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function finiteNumber(value, fallback) {
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

function normalizeCamera(camera) {
  return {
    x: finiteNumber(camera?.x, 0),
    y: finiteNumber(camera?.y, 0),
    scale: clamp(finiteNumber(camera?.scale, 1), 0.1, 3),
  };
}

function wheelDelta(event) {
  const lineHeight = 16;
  const pageHeight = 800;
  const unit =
    event.deltaMode === 1 ? lineHeight : event.deltaMode === 2 ? pageHeight : 1;

  return {
    x: finiteNumber(event.deltaX, 0) * unit,
    y: finiteNumber(event.deltaY, 0) * unit,
  };
}

export function createBoardViewport({
  viewportElement,
  worldElement,
  onCameraChanged,
}) {
  const Panzoom = window.Panzoom;
  if (typeof Panzoom !== "function") {
    throw new Error("Panzoom dependency did not initialize.");
  }

  const panzoom = Panzoom(worldElement, {
    canvas: true,
    cursor: "grab",
    excludeClass: "panzoom-exclude",
    maxScale: 3,
    minScale: 0.1,
    origin: "0 0",
    overflow: "hidden",
    roundPixels: true,
    step: 0.12,
  });

  function cameraFromPanzoom() {
    const pan = panzoom.getPan();
    return {
      x: finiteNumber(pan.x, 0),
      y: finiteNumber(pan.y, 0),
      scale: panzoom.getScale(),
    };
  }

  function emitCameraChanged() {
    if (typeof onCameraChanged === "function") {
      onCameraChanged(cameraFromPanzoom());
    }
  }

  function setCamera(camera, options = {}) {
    const next = normalizeCamera(camera);
    panzoom.zoom(next.scale, {
      animate: options.animate === true,
      force: true,
      silent: options.silent === true,
    });
    panzoom.pan(next.x, next.y, {
      animate: options.animate === true,
      force: true,
      silent: options.silent === true,
    });
  }

  function centerWorldPoint() {
    const rect = viewportElement.getBoundingClientRect();
    const camera = cameraFromPanzoom();
    const scale = clamp(camera.scale, 0.1, 3);

    return {
      x: -camera.x + rect.width / 2 / scale,
      y: -camera.y + rect.height / 2 / scale,
    };
  }

  function worldPointFromClient(clientX, clientY) {
    const rect = viewportElement.getBoundingClientRect();
    const camera = cameraFromPanzoom();
    const scale = clamp(camera.scale, 0.1, 3);

    return {
      x:
        -camera.x +
        (finiteNumber(clientX, rect.left + rect.width / 2) - rect.left) /
          scale,
      y:
        -camera.y +
        (finiteNumber(clientY, rect.top + rect.height / 2) - rect.top) / scale,
    };
  }

  function cameraForCenter(center, scale = panzoom.getScale()) {
    const rect = viewportElement.getBoundingClientRect();
    const safeScale = clamp(finiteNumber(scale, 1), 0.1, 3);

    return {
      x: -finiteNumber(center?.x, 0) + rect.width / 2 / safeScale,
      y: -finiteNumber(center?.y, 0) + rect.height / 2 / safeScale,
      scale: safeScale,
    };
  }

  function setCenteredCamera(center, scale, options = {}) {
    setCamera(cameraForCenter(center, scale), options);
    if (options.silent !== true) {
      emitCameraChanged();
    }
  }

  function zoomBy(factor) {
    const center = centerWorldPoint();
    const scale = clamp(panzoom.getScale() * finiteNumber(factor, 1), 0.1, 3);
    setCenteredCamera(center, scale, { animate: true });
  }

  function panByScreenDelta(deltaX, deltaY) {
    const camera = cameraFromPanzoom();
    const scale = clamp(camera.scale, 0.1, 3);
    setCamera(
      {
        ...camera,
        x: camera.x - finiteNumber(deltaX, 0) / scale,
        y: camera.y - finiteNumber(deltaY, 0) / scale,
      },
      { silent: true },
    );
    emitCameraChanged();
  }

  function handleWheel(event) {
    if (event.defaultPrevented) {
      return;
    }

    event.preventDefault();
    const delta = wheelDelta(event);

    if (event.ctrlKey || event.metaKey) {
      const anchor = worldPointFromClient(event.clientX, event.clientY);
      const factor = Math.exp(-delta.y * 0.0012);
      const scale = clamp(panzoom.getScale() * factor, 0.1, 3);
      setCenteredCamera(anchor, scale, { animate: false });
      return;
    }

    panByScreenDelta(delta.x * 0.42, delta.y * 0.42);
  }

  function resetZoom() {
    setCenteredCamera(centerWorldPoint(), 1, { animate: true });
  }

  function recenter(world) {
    setCenteredCamera(
      {
        x: finiteNumber(world?.width, 10_000) / 2,
        y: finiteNumber(world?.height, 10_000) / 2,
      },
      panzoom.getScale(),
      { animate: true },
    );
  }

  function setInteractionLocked(locked) {
    panzoom.setOptions({
      disablePan: Boolean(locked),
      disableZoom: Boolean(locked),
    });
  }

  viewportElement.addEventListener("wheel", handleWheel, {
    passive: false,
  });
  worldElement.addEventListener("panzoomchange", emitCameraChanged);

  return {
    setCamera,
    getCamera: cameraFromPanzoom,
    getScale: () => panzoom.getScale(),
    centerWorldPoint,
    zoomBy,
    resetZoom,
    recenter,
    setInteractionLocked,
    destroy() {
      viewportElement.removeEventListener("wheel", handleWheel);
      worldElement.removeEventListener("panzoomchange", emitCameraChanged);
      panzoom.destroy();
    },
  };
}
