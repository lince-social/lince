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

function normalizeWorld(world) {
  return {
    width: Math.max(1, finiteNumber(world?.width, 10_000)),
    height: Math.max(1, finiteNumber(world?.height, 10_000)),
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
      x: (rect.width / 2 - camera.x) / scale,
      y: (rect.height / 2 - camera.y) / scale,
    };
  }

  function worldPointFromClient(clientX, clientY) {
    const rect = viewportElement.getBoundingClientRect();
    const camera = cameraFromPanzoom();
    const scale = clamp(camera.scale, 0.1, 3);

    return {
      x:
        (finiteNumber(clientX, rect.left + rect.width / 2) -
          rect.left -
          camera.x) /
        scale,
      y:
        (finiteNumber(clientY, rect.top + rect.height / 2) -
          rect.top -
          camera.y) /
        scale,
    };
  }

  function cameraForCenter(center, scale = panzoom.getScale()) {
    const rect = viewportElement.getBoundingClientRect();
    const safeScale = clamp(finiteNumber(scale, 1), 0.1, 3);

    return {
      x: rect.width / 2 - finiteNumber(center?.x, 0) * safeScale,
      y: rect.height / 2 - finiteNumber(center?.y, 0) * safeScale,
      scale: safeScale,
    };
  }

  function cameraForWorldCenter(world, scale = 1) {
    const normalized = normalizeWorld(world);
    return cameraForCenter(
      {
        x: normalized.width / 2,
        y: normalized.height / 2,
      },
      scale,
    );
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
        x: camera.x - finiteNumber(deltaX, 0),
        y: camera.y - finiteNumber(deltaY, 0),
      },
      { silent: true },
    );
    emitCameraChanged();
  }

  function cardBlocksWheel(excludedCard) {
    // A card opts out of canvas panning so its own content (e.g. a widget
    // iframe) can handle the wheel natively. But in edit mode the widget
    // iframe is set to pointer-events: none so card-dragging takes
    // priority, which means it can never actually receive that wheel
    // event - the browser routes it to the card underneath instead. In
    // that case there's nothing to hand the scroll to, so fall through
    // and let the canvas pan rather than silently dropping the input.
    const frame = excludedCard.querySelector(".package-widget__frame");
    if (!frame) {
      return true;
    }

    return getComputedStyle(frame).pointerEvents !== "none";
  }

  function handleWheel(event) {
    if (event.defaultPrevented) {
      return;
    }

    const excludedCard = event.target?.closest?.(".panzoom-exclude");
    if (excludedCard && cardBlocksWheel(excludedCard)) {
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
    setCamera(cameraForWorldCenter(world, panzoom.getScale()), {
      animate: true,
    });
    emitCameraChanged();
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
    worldPointFromClient,
    cameraForWorldCenter,
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
