const DEFAULT_WORLD = {
  width: 10_000,
  height: 10_000,
  snap: 40,
};

const DEFAULT_CARD_SIZE = {
  width: 640,
  height: 420,
};

export const MIN_CARD_SIZE = {
  width: 240,
  height: 180,
};

const SNAP_PRESETS = [
  { level: 1, label: "livre", snap: 80 },
  { level: 2, label: "largo", snap: 64 },
  { level: 3, label: "medio", snap: 48 },
  { level: 4, label: "padrao", snap: 40 },
  { level: 5, label: "fino", snap: 32 },
  { level: 6, label: "preciso", snap: 24 },
  { level: 7, label: "micro", snap: 16 },
];

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function finiteNumber(value, fallback) {
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

function cloneJsonValue(value, fallback = {}) {
  try {
    if (value == null) {
      return fallback;
    }

    return JSON.parse(JSON.stringify(value));
  } catch {
    return fallback;
  }
}

function snapValue(value, snap) {
  const step = Math.max(1, finiteNumber(snap, DEFAULT_WORLD.snap));
  return Math.round(value / step) * step;
}

function sanitizePermissions(rawPermissions) {
  return Array.isArray(rawPermissions)
    ? rawPermissions
        .map((permission) => String(permission).trim())
        .filter(Boolean)
    : [];
}

function sanitizeWidgetState(rawWidgetState) {
  return rawWidgetState == null ? {} : cloneJsonValue(rawWidgetState, {});
}

function cardSizeFromRaw(rawCard) {
  const width =
    rawCard?.width ??
    rawCard?.initialWidth ??
    rawCard?.initial_width ??
    (rawCard?.w == null ? null : Number(rawCard.w) * 180);
  const height =
    rawCard?.height ??
    rawCard?.initialHeight ??
    rawCard?.initial_height ??
    (rawCard?.h == null ? null : Number(rawCard.h) * 160);

  return {
    width: finiteNumber(width, DEFAULT_CARD_SIZE.width),
    height: finiteNumber(height, DEFAULT_CARD_SIZE.height),
  };
}

export function clampDensityLevel(level) {
  return clamp(Math.round(Number(level) || 4), 1, SNAP_PRESETS.length);
}

export function getDensityPreset(level) {
  return SNAP_PRESETS[clampDensityLevel(level) - 1];
}

export function normalizeWorld(rawWorld) {
  return {
    width: clamp(finiteNumber(rawWorld?.width, DEFAULT_WORLD.width), 2_000, 10_000),
    height: clamp(finiteNumber(rawWorld?.height, DEFAULT_WORLD.height), 2_000, 10_000),
    snap: clamp(finiteNumber(rawWorld?.snap, DEFAULT_WORLD.snap), 4, 200),
  };
}

export function defaultCamera(world = DEFAULT_WORLD) {
  return {
    // Seed workspace: centered at 100% for a 1920×1080 canvas.
    x: -Math.max(0, finiteNumber(world.width, DEFAULT_WORLD.width) / 2 - 960),
    y: -Math.max(0, finiteNumber(world.height, DEFAULT_WORLD.height) / 2 - 540),
    scale: 1,
  };
}

export function sanitizeCamera(rawCamera, world) {
  const fallback = defaultCamera(world);
  const x = finiteNumber(rawCamera?.x, fallback.x);
  const y = finiteNumber(rawCamera?.y, fallback.y);

  if (Math.abs(x) > world.width || Math.abs(y) > world.height) {
    return fallback;
  }

  return {
    x,
    y,
    scale: clamp(finiteNumber(rawCamera?.scale, fallback.scale), 0.1, 3),
  };
}

export function applyDensity(config, level) {
  const preset = getDensityPreset(level);

  config.density = preset.level;
  config.densityLabel = preset.label;
  config.world = {
    ...normalizeWorld(config.world),
    snap: preset.snap,
  };

  return config;
}

export function createGridConfig(raw) {
  const world = normalizeWorld(raw?.boardState?.world || raw?.world);
  return {
    density: clampDensityLevel(raw?.density || raw?.boardState?.density || 4),
    densityLabel: getDensityPreset(raw?.density || raw?.boardState?.density || 4).label,
    world,
  };
}

export function viewportCenterWorldPoint(viewportElement, camera, world) {
  const rect = viewportElement?.getBoundingClientRect?.() || {
    width: 1600,
    height: 1000,
  };
  const scale = clamp(finiteNumber(camera?.scale, 1), 0.1, 3);

  return {
    x: clamp((-finiteNumber(camera?.x, 0) + rect.width / 2) / scale, 0, world.width),
    y: clamp((-finiteNumber(camera?.y, 0) + rect.height / 2) / scale, 0, world.height),
  };
}

export function sanitizeCard(rawCard, index, config, placementPoint = null) {
  const world = normalizeWorld(config.world);
  const kind = rawCard?.kind === "package" ? "package" : "text";
  const size = cardSizeFromRaw(rawCard);
  const pinned = rawCard?.pinned === true;
  const rawX = rawCard?.x == null ? null : finiteNumber(rawCard.x, null);
  const rawY = rawCard?.y == null ? null : finiteNumber(rawCard.y, null);
  const rawPositionFitsWorld =
    pinned ||
    rawX != null &&
    rawY != null &&
    rawX >= 0 &&
    rawY >= 0 &&
    rawX <= world.width &&
    rawY <= world.height;
  const fallbackPoint = placementPoint || {
    x: world.width / 2 + index * world.snap,
    y: world.height / 2 + index * world.snap,
  };

  return clampCard(
    {
      id: String(rawCard?.id || `card-${index + 1}`),
      kind,
      title: String(rawCard?.title || `Bloco ${index + 1}`),
      description: String(
        rawCard?.description ||
          "Card base pronto para receber tabela, formulario ou mini app.",
      ),
      text:
        kind === "package"
          ? String(rawCard?.text || "")
          : String(
              rawCard?.text ||
                "Card base pronto para receber tabela, formulario, status ou outro mini app.",
            ),
      html: kind === "package" ? String(rawCard?.html || "") : "",
      author: kind === "package" ? String(rawCard?.author || "") : "",
      permissions: sanitizePermissions(rawCard?.permissions),
      packageName: kind === "package" ? String(rawCard?.packageName || "") : "",
      requiresServer: kind === "package" ? rawCard?.requiresServer === true : false,
      serverId: kind === "package" ? String(rawCard?.serverId || "") : "",
      viewId:
        kind === "package" && rawCard?.viewId != null
          ? Number(rawCard.viewId) || null
          : null,
      streamsEnabled:
        kind === "package" ? rawCard?.streamsEnabled !== false : true,
      widgetState:
        kind === "package" ? sanitizeWidgetState(rawCard?.widgetState) : {},
      x: rawPositionFitsWorld ? rawX : fallbackPoint.x - size.width / 2,
      y: rawPositionFitsWorld ? rawY : fallbackPoint.y - size.height / 2,
      width: size.width,
      height: size.height,
      pinned,
      system: rawCard?.system === true,
      zIndex: Math.round(finiteNumber(rawCard?.zIndex, pinned ? 50 : 1)),
      groupId: rawCard?.groupId ? String(rawCard.groupId) : null,
      groupIds: Array.isArray(rawCard?.groupIds)
        ? rawCard.groupIds.map((id) => String(id)).filter(Boolean)
        : [],
      abiListen: sanitizePermissions(rawCard?.abiListen),
    },
    config,
  );
}

export function clampCard(card, config) {
  const world = normalizeWorld(config.world);
  if (card?.pinned === true) {
    return {
      ...card,
      x: finiteNumber(card.x, 0),
      y: finiteNumber(card.y, 0),
      width: clamp(
        finiteNumber(card.width, DEFAULT_CARD_SIZE.width),
        48,
        Math.max(48, window.innerWidth || 1920),
      ),
      height: clamp(
        finiteNumber(card.height, DEFAULT_CARD_SIZE.height),
        40,
        Math.max(40, window.innerHeight || 1080),
      ),
      zIndex: Math.round(finiteNumber(card.zIndex, 50)),
    };
  }
  const width = clamp(
    finiteNumber(card.width, DEFAULT_CARD_SIZE.width),
    MIN_CARD_SIZE.width,
    world.width,
  );
  const height = clamp(
    finiteNumber(card.height, DEFAULT_CARD_SIZE.height),
    MIN_CARD_SIZE.height,
    world.height,
  );
  const x = clamp(
    finiteNumber(card.x, world.width / 2 - width / 2),
    0,
    Math.max(0, world.width - width),
  );
  const y = clamp(
    finiteNumber(card.y, world.height / 2 - height / 2),
    0,
    Math.max(0, world.height - height),
  );

  return {
    ...card,
    x,
    y,
    width,
    height,
  };
}

export function normalizeLayout(cards, config) {
  return cards.map((card, index) => sanitizeCard(card, index, config));
}

export function findOpenPosition(cards, size, config, centerPoint = null) {
  const world = normalizeWorld(config.world);
  const width = finiteNumber(size?.width ?? size?.w, DEFAULT_CARD_SIZE.width);
  const height = finiteNumber(size?.height ?? size?.h, DEFAULT_CARD_SIZE.height);
  const center = centerPoint || {
    x: world.width / 2,
    y: world.height / 2,
  };
  const offset = cards.length * world.snap;

  return clampCard(
    {
      x: center.x - width / 2 + offset,
      y: center.y - height / 2 + offset,
      width,
      height,
    },
    config,
  );
}

function cardsOverlap(a, b, margin) {
  return !(
    a.x + a.width + margin <= b.x ||
    b.x + b.width + margin <= a.x ||
    a.y + a.height + margin <= b.y ||
    b.y + b.height + margin <= a.y
  );
}

export function arrangeCardsInCircle(cards, centerPoint, config) {
  const world = normalizeWorld(config.world);
  const center = centerPoint || {
    x: world.width / 2,
    y: world.height / 2,
  };
  const snap = Math.max(1, world.snap);
  const margin = Math.max(12, snap / 2);
  const goldenAngle = Math.PI * (3 - Math.sqrt(5));
  const sorted = cards
    .map((card, index) => ({
      card,
      index,
      area: Number(card.width) * Number(card.height),
    }))
    .sort((a, b) => b.area - a.area);
  const placed = [];
  const nextById = new Map();

  for (const entry of sorted) {
    const card = entry.card;
    const width = finiteNumber(card.width, DEFAULT_CARD_SIZE.width);
    const height = finiteNumber(card.height, DEFAULT_CARD_SIZE.height);
    let selected = null;

    if (!placed.length) {
      selected = {
        x: center.x - width / 2,
        y: center.y - height / 2,
        width,
        height,
      };
    }

    for (let ring = 1; !selected && ring <= 80; ring += 1) {
      const radius = ring * snap * 2;
      const slots = Math.max(
        8,
        Math.ceil((Math.PI * 2 * radius) / Math.max(snap * 3, Math.max(width, height) / 2)),
      );

      for (let slot = 0; slot < slots; slot += 1) {
        const angle = entry.index * goldenAngle + (slot / slots) * Math.PI * 2;
        const candidate = {
          x: center.x + Math.cos(angle) * radius - width / 2,
          y: center.y + Math.sin(angle) * radius - height / 2,
          width,
          height,
        };

        if (!placed.some((other) => cardsOverlap(candidate, other, margin))) {
          selected = candidate;
          break;
        }
      }
    }

    const arranged = clampCard(
      {
        ...card,
        ...(selected || {
          x: center.x - width / 2,
          y: center.y - height / 2,
          width,
          height,
        }),
      },
      config,
    );
    placed.push(arranged);
    nextById.set(card.id, arranged);
  }

  return cards.map((card) => nextById.get(card.id) || card);
}

export function screenDeltaToWorldDelta(moveX, moveY, scale) {
  const safeScale = clamp(finiteNumber(scale, 1), 0.1, 3);
  return {
    x: moveX / safeScale,
    y: moveY / safeScale,
  };
}

export function buildMoveCandidate(card, delta, config) {
  return clampCard(
    {
      ...card,
      x: card.x + delta.x,
      y: card.y + delta.y,
    },
    config,
  );
}

export function buildResizeCandidate(card, handle, delta, config) {
  let left = card.x;
  let right = card.x + card.width;
  let top = card.y;
  let bottom = card.y + card.height;

  if (handle.includes("w")) {
    left += delta.x;
  }
  if (handle.includes("e")) {
    right += delta.x;
  }
  if (handle.includes("n")) {
    top += delta.y;
  }
  if (handle.includes("s")) {
    bottom += delta.y;
  }

  const next = {
    ...card,
    x: Math.min(left, right - MIN_CARD_SIZE.width),
    y: Math.min(top, bottom - MIN_CARD_SIZE.height),
    width: Math.max(MIN_CARD_SIZE.width, right - left),
    height: Math.max(MIN_CARD_SIZE.height, bottom - top),
  };

  return clampCard(next, config);
}
