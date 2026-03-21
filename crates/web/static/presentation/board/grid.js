function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function cloneCard(card) {
  return { ...card };
}

function sanitizePermissions(rawPermissions) {
  return Array.isArray(rawPermissions)
    ? rawPermissions.map((permission) => String(permission).trim()).filter(Boolean)
    : [];
}

const DENSITY_PRESETS = [
  { level: 1, label: "solto", cols: 10, rows: 7, gap: 18 },
  { level: 2, label: "aberto", cols: 12, rows: 8, gap: 16 },
  { level: 3, label: "padrao", cols: 14, rows: 9, gap: 14 },
  { level: 4, label: "fino", cols: 16, rows: 10, gap: 12 },
  { level: 5, label: "denso", cols: 18, rows: 11, gap: 10 },
  { level: 6, label: "compacto", cols: 20, rows: 12, gap: 9 },
  { level: 7, label: "micro", cols: 22, rows: 13, gap: 8 },
];

export function clampDensityLevel(level) {
  return clamp(Math.round(Number(level) || 3), 1, DENSITY_PRESETS.length);
}

export function getDensityPreset(level) {
  return DENSITY_PRESETS[clampDensityLevel(level) - 1];
}

export function applyDensity(config, level) {
  const preset = getDensityPreset(level);

  config.density = preset.level;
  config.densityLabel = preset.label;
  config.cols = preset.cols;
  config.rows = preset.rows;
  config.gap = preset.gap;

  return config;
}

export function createGridConfig(raw) {
  const config = {
    cols: Number(raw.cols) || 16,
    rows: Number(raw.rows) || 10,
    gap: Number(raw.gap) || 12,
    density: clampDensityLevel(raw.density || 4),
    densityLabel: "fino",
  };

  return applyDensity(config, config.density);
}

export function sanitizeCard(rawCard, index, config) {
  const kind = rawCard?.kind === "package" ? "package" : "text";
  const title = String(rawCard?.title || `Resumo ${index + 1}`);
  const description = String(
    rawCard?.description || "Card base pronto para receber tabela, formulario ou mini app.",
  );
  const text =
    kind === "package"
      ? String(rawCard?.text || "")
      : String(
          rawCard?.text ||
            "Card base pronto para receber tabela, formulario, status ou outro mini app.",
        );

  return clampCard(
    {
      id: String(rawCard?.id || `card-${index + 1}`),
      kind,
      title,
      description,
      text,
      html: kind === "package" ? String(rawCard?.html || "") : "",
      author: kind === "package" ? String(rawCard?.author || "") : "",
      permissions: sanitizePermissions(rawCard?.permissions),
      packageName: kind === "package" ? String(rawCard?.packageName || "") : "",
      x: Number(rawCard?.x) || 1,
      y: Number(rawCard?.y) || 1,
      w: Number(rawCard?.w) || 3,
      h: Number(rawCard?.h) || 2,
    },
    config,
  );
}

export function clampCard(card, config) {
  const w = clamp(Math.round(card.w), 1, config.cols);
  const h = clamp(Math.round(card.h), 1, config.rows);
  const x = clamp(Math.round(card.x), 1, config.cols - w + 1);
  const y = clamp(Math.round(card.y), 1, config.rows - h + 1);

  return {
    ...cloneCard(card),
    x,
    y,
    w,
    h,
  };
}

export function cardsOverlap(a, b) {
  return (
    a.x < b.x + b.w &&
    a.x + a.w > b.x &&
    a.y < b.y + b.h &&
    a.y + a.h > b.y
  );
}

export function isAreaAvailable(cards, candidate, config, ignoreId = null) {
  const next = clampCard(candidate, config);

  return cards
    .filter((card) => card.id !== ignoreId)
    .every((card) => !cardsOverlap(next, card));
}

export function findOpenPosition(cards, size, config) {
  const w = clamp(Math.round(size.w), 1, config.cols);
  const h = clamp(Math.round(size.h), 1, config.rows);

  for (let y = 1; y <= config.rows - h + 1; y += 1) {
    for (let x = 1; x <= config.cols - w + 1; x += 1) {
      const candidate = {
        id: "__candidate__",
        kind: "text",
        title: "",
        description: "",
        text: "",
        html: "",
        author: "",
        permissions: [],
        packageName: "",
        x,
        y,
        w,
        h,
      };
      if (isAreaAvailable(cards, candidate, config, candidate.id)) {
        return { x, y, w, h };
      }
    }
  }

  return null;
}

export function normalizeLayout(cards, config) {
  const placed = [];

  for (const [index, rawCard] of cards.entries()) {
    const preferred = sanitizeCard(rawCard, index, config);
    const position = isAreaAvailable(placed, preferred, config, preferred.id)
      ? preferred
      : findOpenPosition(placed, preferred, config);

    if (!position) {
      continue;
    }

    placed.push({
      ...preferred,
      ...position,
    });
  }

  return placed;
}

export function measureBoard(boardElement, config) {
  const rect = boardElement.getBoundingClientRect();
  const cellWidth = (rect.width - (config.cols - 1) * config.gap) / config.cols;
  const cellHeight = (rect.height - (config.rows - 1) * config.gap) / config.rows;

  return {
    rect,
    cellWidth,
    cellHeight,
    stepX: cellWidth + config.gap,
    stepY: cellHeight + config.gap,
  };
}

export function deltaToGrid(moveX, moveY, metrics) {
  return {
    cols: Math.round(moveX / metrics.stepX),
    rows: Math.round(moveY / metrics.stepY),
  };
}

export function buildMoveCandidate(card, delta, config) {
  return clampCard(
    {
      ...card,
      x: card.x + delta.cols,
      y: card.y + delta.rows,
    },
    config,
  );
}

export function buildResizeCandidate(card, handle, delta, config) {
  let left = card.x;
  let right = card.x + card.w - 1;
  let top = card.y;
  let bottom = card.y + card.h - 1;

  if (handle.includes("w")) {
    left += delta.cols;
  }
  if (handle.includes("e")) {
    right += delta.cols;
  }
  if (handle.includes("n")) {
    top += delta.rows;
  }
  if (handle.includes("s")) {
    bottom += delta.rows;
  }

  left = clamp(left, 1, config.cols);
  right = clamp(right, 1, config.cols);
  top = clamp(top, 1, config.rows);
  bottom = clamp(bottom, 1, config.rows);

  if (left > right) {
    if (handle.includes("w")) {
      left = right;
    } else {
      right = left;
    }
  }

  if (top > bottom) {
    if (handle.includes("n")) {
      top = bottom;
    } else {
      bottom = top;
    }
  }

  return clampCard(
    {
      ...card,
      x: left,
      y: top,
      w: right - left + 1,
      h: bottom - top + 1,
    },
    config,
  );
}
