The Web interface is an HTML based one. It can be run in browsers or as a desktop app with Tauri.

The base app should be minimalist to give as much space as possible for user's to express themselves. That expression should feel familiar, reflecting what they want.

In addition to what we say is the base app we have possibly many components, widgets, called 'Sand'. They are HTML iframes inside a canvas, so like blocks of lego in a whiteboard. We have an edit mode for being able to move components around, add or remove them, including many other actions.

# Design System

Vibe: Minimalist and friendly.
Description:
Lince Design System — Description

   What the docs demand of it

   Five properties of Lince dictate the visual language before any color is picked:

   • "No dashboard." The north star is Ana spending under four minutes, all of it on decisions only a human can make. The UI's job is to disappear. Attention is the scarcest resource in the
     system, so color, motion and elevation are spent, never decorated with.
   • Numbers are the protagonist. Everything is a Record with a quantity; negative = Need, positive = Contribution, zero = peace. The typography of numbers matters more than any chrome.
   • Honesty over decoration. "A number on a chart that nobody can explain is worse than no number." Surfaces are opaque, line styles carry truth (settled vs. declared), color never carries
     meaning alone.
   • A whiteboard, not a cockpit. Sand are lego blocks on a bullet-journal canvas — dots, hand-drawn arrows, blocks the user arranges. Familiar, paper-like, user-owned.
   • The brand is monochrome. The lynx logo exists only as black-on-white and white-on-black. Lince has no corporate color — which means the palette is free to be warm and paper-like instead
     of "brand blue," and users' own colorschemes are first-class citizens.

   The palette: 4 families, 80/20

   Roughly 80% of every screen comes from one family of closely-related neutrals; 20% is highlights split across three small families. Four families total:

   1. Paper (≈80% of pixels). One hue family of 6–7 closely-related warm neutrals forming a ramp: canvas → surface → raised → border → ink-secondary → ink-primary. Warm grey, paper-and-ink,
   not blue-grey. All chrome, cards, text, and structure live here. Dark mode is the same ramp inverted (warm charcoal, not pure black). Adjacent steps must pass WCAG AA for their intended
   pairings. If a screen shows more than one hue family at a glance, something is wrong.

   2. Meaning (semantic, scheme-invariant). The quantity trio: Need (terracotta/warm red), Contribution (sage/green), Peace — zero borrows a neutral from Paper, because zero is rest, and
   rest shouldn't glow. These hues may shift per colorscheme but their assignment is locked, and they only ever appear attached to data (a signed number, a delta), always with the −/+ sign
   or a label — never color alone.

   3. Attention (the 20% highlight budget). One interactive accent — default a warm ochre/amber, the lynx's fur, the only warm-saturated hue the system owns — used for focus rings,
   selection, primary actions, and the "cute little ball" connection indicator. Beside it, a desaturated status trio (info/success/warning/danger) used exclusively for live state. Nothing in
   this family may be decorative; a screen full of amber is a design bug, because it means the tool is shouting, and Lince whispers.

   4. Expression (user-owned). Organ identity colors, presence cursors, kanban column washes, the Lincegoshi. Saturated, playful, entirely chosen by the user or their colorscheme — system
   chrome never touches this family. This is where "blocks of lego" personality lives: the user's canvas is colorful; Lince's frame around it is not.

   Space, thickness, roundness, rigidity

   • Grid: 4px base unit; spacing scale 4, 8, 12, 16, 24, 32, 48. The canvas renders a bullet-journal dot grid (dots every ~24px) that Sand snap to in edit mode.
   • Density: comfortable by default — 16px padding inside cards, 8px gaps between elements; tables and ledgers may go compact (12px/4px) because they're reading surfaces, not touch
     surfaces.
   • Borders: 1px hairlines everywhere — printed-rule crispness. 2px reserved for focus and active state. No double borders, no nested boxes boxing boxes.
   • Roundness: friendly but firm. 8px on Sand cards and panels, 4px on inputs and buttons, pill only for chips, tags, and the status ball. It should feel like a well-used notebook, not a
     bubble UI.
   • Rigidity: "firm paper." Cards hold their shape with crisp hairline borders; nothing bounces, nothing elastic — except the lynx. Playfulness is concentrated in the mascot and the user's
     Expression colors, not in the chrome's physics.

   Transparency and elevation

   • Data surfaces are always opaque. Honesty rule: you must always know exactly which surface a number sits on. No glassmorphism, no frosted panels over content.
   • Translucency is allowed only for ephemeral chrome: edit-mode handles, drag previews, presence cursors, auto-hiding call UI — things that are explicitly not Ledger truth.
   • Overlays/scrims at 50–60% ink. Menus and tooltips fully opaque with a hairline border.
   • Elevation is nearly flat: at most one soft shadow (0 1px 3px at ~10% black) for floating layers. Paper doesn't hover.

   Line, shape, and texture grammar

   A deliberate second channel so color is never the only carrier:

   • Solid = settled (Ledger facts, committed quantities). Dashed = declared (promises, projections, staged rules). This is as load-bearing as any hue.
   • The dot grid, hand-drawn arrows/boxes/text on the canvas, and the monochrome flat lynx mark set the shape language: line icons at 1.5–2px stroke, rounded caps, single-color.

   Typography

   • Numbers first: tabular figures everywhere quantities appear, quantities always signed (−3, 0, +5), and the minus rendered as a true minus. The zero state is styled quietly — peace is
     the one value that should never demand attention.
   • UI text in a humanist sans at comfortable sizes; EN/PT bilingual from day one, so generous line lengths and no cramped all-caps labels.

   Motion

   120–200ms ease-out for state changes, ~300ms for Sand moves in edit mode. No looping animations, no pulsing badges, nothing that runs while the user isn't acting. prefers-reduced-motion
   collapses everything to fades. The single sanctioned exception to all of this restraint is the lynx itself — the website lynx that gets hyperactive when petted is the brand's chaos valve,
   and the design system deliberately leaves it as the only wild element.

   Architecture (from Sand: Colorschemes)

   The system is defined as named semantic tokens, not hex values — surface-raised, ink-primary, need, contribution, accent, focus — designed as Figma variables, exported, and resolved per
   active colorscheme at runtime (the old a2 Operation to switch schemes is the spiritual ancestor). Scaling tokens (padding-s, radius-m) ride the same mechanism. A Sand author never picks a
   color; they name a slot, and the user's scheme decides what it looks like. That's how "the base app is minimalist so users can express themselves" survives contact with real widgets.

   The test

   Every design decision gets one question: does this get Ana to her four minutes of human choice faster, or is it the tool asking to be looked at? The Death of Lince applies to its UI
   first.

- [ ] Color Variables
- [ ] Icon Library
- [ ] Chart Library


# Coding
- [ ] Background with dots pattern like a bullet-journal.
- [ ] Be able to draw (arrows, boxes, text and erasing at first is ok).

## [x] Wire protocol — how a sand talks to the Cell

- [x] One WebSocket (`/host/transport/ws`), multiplexed: Protein (reads) +
  Actions (writes) + ephemeral lanes (presence/cursors/events) + explicit
  host capabilities (e.g. a terminal PTY session) whose bytes don't belong in
  the Ledger.
- [x] Actions are JSON with a kebab-case `"action"` tag, snake_case
  everywhere else; Protein predicates/includes are snake_case too.
- [x] A subscription answers with a snapshot then re-executes and pushes on
  every relevant commit; invalidation is coarse-by-source — render
  idempotently, a sand may get refreshes it doesn't strictly need.
- [x] Action responses carry `created`, `facts` (what the Ledger committed,
  including any Karma cascade), and `warnings` (non-fatal advisories) — show
  warnings, never treat them as errors.
- [x] Ephemeral-lane and host-capability traffic (cursors, clicks, presence,
  PTY bytes) is never persisted; terminal PTYs are scoped to one connection
  and die with it.
