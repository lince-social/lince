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

function isPointInside(rect, clientX, clientY) {
  return (
    clientX >= rect.left &&
    clientX <= rect.right &&
    clientY >= rect.top &&
    clientY <= rect.bottom
  );
}

function isExcludedTarget(target) {
  return Boolean(target?.closest?.(".panzoom-exclude"));
}

export function createBoardViewport({
  viewportElement,
  worldElement,
  onCameraChanged,
}) {
  let camera = normalizeCamera({ x: 0, y: 0, scale: 1 });
  let interactionLocked = false;
  let spacePanMode = false;
  let activeGesture = null;
  let emitFrame = 0;

  worldElement.style.transformOrigin = "0 0";
  worldElement.style.willChange = "transform";

  function cameraSnapshot() {
    return { ...camera };
  }

  function applyTransform() {
    worldElement.style.transform = `translate3d(${camera.x}px, ${camera.y}px, 0) scale(${camera.scale})`;
  }

  function emitCameraChanged() {
    emitFrame = 0;
    const snapshot = cameraSnapshot();
    if (typeof onCameraChanged === "function") {
      onCameraChanged(snapshot);
    }
    worldElement.dispatchEvent(
      new CustomEvent("panzoomchange", {
        detail: snapshot,
      }),
    );
  }

  function scheduleCameraChanged() {
    if (emitFrame) {
      return;
    }

    emitFrame = requestAnimationFrame(emitCameraChanged);
  }

  function assignCamera(nextCamera, options = {}) {
    camera = normalizeCamera(nextCamera);
    applyTransform();
    if (options.silent !== true) {
      scheduleCameraChanged();
    }
  }

  function setCamera(nextCamera, options = {}) {
    assignCamera(nextCamera, options);
  }

  function centerWorldPoint() {
    const rect = viewportElement.getBoundingClientRect();
    const scale = clamp(camera.scale, 0.1, 3);

    return {
      x: (rect.width / 2 - camera.x) / scale,
      y: (rect.height / 2 - camera.y) / scale,
    };
  }

  function worldPointFromClient(clientX, clientY) {
    const rect = viewportElement.getBoundingClientRect();
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

  function cameraForCenter(center, scale = camera.scale) {
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
    assignCamera(cameraForCenter(center, scale), options);
  }

  function zoomBy(factor) {
    const center = centerWorldPoint();
    const scale = clamp(camera.scale * finiteNumber(factor, 1), 0.1, 3);
    setCenteredCamera(center, scale);
  }

  function panByScreenDelta(deltaX, deltaY) {
    assignCamera({
      ...camera,
      x: camera.x - finiteNumber(deltaX, 0),
      y: camera.y - finiteNumber(deltaY, 0),
    });
  }

  function cardBlocksWheel(excludedCard) {
    const frame = excludedCard.querySelector(".package-widget__frame");
    if (!frame) {
      return true;
    }

    return getComputedStyle(frame).pointerEvents !== "none";
  }

  function handleWheel(event) {
    if (interactionLocked || event.defaultPrevented) {
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
      const scale = clamp(camera.scale * factor, 0.1, 3);
      const rect = viewportElement.getBoundingClientRect();
      assignCamera({
        x: finiteNumber(event.clientX, rect.left) - rect.left - anchor.x * scale,
        y: finiteNumber(event.clientY, rect.top) - rect.top - anchor.y * scale,
        scale,
      });
      return;
    }

    panByScreenDelta(delta.x * 0.42, delta.y * 0.42);
  }

  function resetZoom() {
    setCenteredCamera(centerWorldPoint(), 1);
  }

  function recenter(world) {
    assignCamera(cameraForWorldCenter(world, camera.scale));
  }

  function setInteractionLocked(locked) {
    interactionLocked = Boolean(locked);
    if (interactionLocked) {
      endGesture();
    }
  }

  function setSpacePanMode(enabled) {
    spacePanMode = Boolean(enabled);
    if (!spacePanMode) {
      endGesture();
    }
  }

  function canStartNormalPan(event) {
    return (
      !interactionLocked &&
      !spacePanMode &&
      // Ctrl/meta + drag is the marquee group-selection gesture; this handler
      // runs in window capture before the board's own listeners, so it must
      // yield or the marquee never receives the pointerdown.
      !event.ctrlKey &&
      !event.metaKey &&
      viewportElement.contains(event.target) &&
      !isExcludedTarget(event.target)
    );
  }

  function canStartSpacePan(event) {
    if (!spacePanMode || interactionLocked) {
      return false;
    }

    return isPointInside(
      viewportElement.getBoundingClientRect(),
      event.clientX,
      event.clientY,
    );
  }

  function capturePointer(event) {
    const target =
      event.target && typeof event.target.setPointerCapture === "function"
        ? event.target
        : viewportElement;
    try {
      target.setPointerCapture?.(event.pointerId);
      return target;
    } catch {
      return null;
    }
  }

  function startGesture(event, mode) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation?.();

    activeGesture = {
      mode,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      camera: cameraSnapshot(),
      captureTarget: capturePointer(event),
    };

    window.addEventListener("pointermove", handlePointerMove, {
      passive: false,
      capture: true,
    });
    window.addEventListener("pointerrawupdate", handlePointerMove, {
      passive: false,
      capture: true,
    });
    window.addEventListener("pointerup", handlePointerUp, true);
    window.addEventListener("pointercancel", handlePointerUp, true);
  }

  function handlePointerDown(event) {
    if (
      activeGesture ||
      event.defaultPrevented ||
      event.button !== 0 ||
      !isPointInside(
        viewportElement.getBoundingClientRect(),
        event.clientX,
        event.clientY,
      )
    ) {
      return;
    }

    if (canStartSpacePan(event)) {
      startGesture(event, "space");
      return;
    }

    if (canStartNormalPan(event)) {
      startGesture(event, "normal");
    }
  }

  function moveGesture(event) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation?.();

    const dx = finiteNumber(event.clientX - activeGesture.startX, 0);
    const dy = finiteNumber(event.clientY - activeGesture.startY, 0);
    camera = {
      ...activeGesture.camera,
      x: activeGesture.camera.x + dx,
      y: activeGesture.camera.y + dy,
    };
    applyTransform();
    scheduleCameraChanged();
  }

  function handlePointerMove(event) {
    if (!activeGesture || event.pointerId !== activeGesture.pointerId) {
      return;
    }

    moveGesture(event);
  }

  function endGesture(event) {
    if (
      event &&
      activeGesture &&
      event.pointerId !== undefined &&
      event.pointerId !== activeGesture.pointerId
    ) {
      return;
    }

    if (activeGesture?.captureTarget) {
      try {
        activeGesture.captureTarget.releasePointerCapture?.(
          activeGesture.pointerId,
        );
      } catch {
        // Pointer capture can already be gone after pointercancel/lost capture.
      }
    }
    activeGesture = null;
    window.removeEventListener("pointermove", handlePointerMove, true);
    window.removeEventListener("pointerrawupdate", handlePointerMove, true);
    window.removeEventListener("pointerup", handlePointerUp, true);
    window.removeEventListener("pointercancel", handlePointerUp, true);
  }

  function handlePointerUp(event) {
    endGesture(event);
  }

  viewportElement.addEventListener("wheel", handleWheel, {
    passive: false,
  });
  window.addEventListener("pointerdown", handlePointerDown, {
    capture: true,
    passive: false,
  });

  applyTransform();

  return {
    setCamera,
    getCamera: cameraSnapshot,
    getScale: () => camera.scale,
    centerWorldPoint,
    worldPointFromClient,
    cameraForWorldCenter,
    zoomBy,
    resetZoom,
    recenter,
    setInteractionLocked,
    setSpacePanMode,
    destroy() {
      viewportElement.removeEventListener("wheel", handleWheel);
      window.removeEventListener("pointerdown", handlePointerDown, true);
      endGesture();
      if (emitFrame) {
        cancelAnimationFrame(emitFrame);
        emitFrame = 0;
      }
    },
  };
}
