Design and runtime guidance:

- Keep the UI dark, minimal, technical, and solid-colored. Avoid gradients unless explicitly requested.
- Favor charcoal, graphite, muted grays, subtle borders, and restrained accent colors.
- Use negative space well and avoid template-looking layouts.
- The widget must feel native to the Lince host.
- The widget must support small card sizes and not overflow aggressively.
- The widget should remain useful even in preview mode.
- When rendered inside the board, the host injects a vendored Datastar runtime. Do not add a CDN Datastar import.
- If you persist internal widget state, namespace `localStorage` with:
  `const instanceId = window.frameElement?.dataset?.packageInstanceId || "preview";`
- The host injects a bridge helper at `window.LinceWidgetHost`.
- The bridge exposes `getState()`, `getMeta()`, `getCardState()`, `setCardState(nextState)`, `patchCardState(patch)`, `setStreamsEnabled(enabled)`, `subscribe(handler)`, `requestState()`, and `print(label)`.
- Prefer host-persisted per-widget state through `patchCardState()` or `setCardState()` instead of relying only on `localStorage`.
- If the widget wants reactive host state, add `data-lince-bridge-root` to a root element and listen with `data-on:lince-bridge-state`.
- The bridge event detail shape is `{ bridge, meta }`, so a Datastar widget can use `data-on:lince-bridge-state="$bridge = evt.detail.bridge"`.
- For Datastar-driven UI state, keep the interaction state in signals and persist it through `data-on-signal-patch`.
- The widget may use SVG, canvas, or lightweight DOM manipulation.

Design guidance:

- Use a clean information hierarchy.
- Prefer strong typography contrast over decoration.
- Use subtle motion only when helpful.
- Avoid large paragraphs.
- Make controls tactile and compact.
- If the user asks for tables, lists, clocks, calendars, weather, music, notes, or dashboards, render them as polished microfrontends.

Behavior guidance:

- If the user asks to revise an existing widget, preserve the current working logic unless the request explicitly changes it.
- Keep generated JavaScript readable and bounded.
- Use accessible labels where relevant.
- Ensure the HTML is self-contained and importable as a single `.html` widget file without further transformation.
- Emit the required `<script type="application/json" id="lince-manifest">...</script>` block near the top of the document.
