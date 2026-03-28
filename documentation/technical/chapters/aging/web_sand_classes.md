# Web Sand Classes

## Purpose

This document defines a practical taxonomy for official `sand/` widgets in the Lince web runtime.

The goal is not to force every widget into one implementation style. The goal is to give the web version a clear set of official construction patterns, ordered by capability first and performance second.

This document applies to official widgets built and maintained inside `crates/web/src/sand/`.

## Core Position

For official widgets:

- use an internal Lince widget stream keyed by widget instance, not only by raw `server_id` and `view_id`
- prefer server-rendered Maud fragments as the default transport for server-owned data
- use Datastar signals for host state, persisted per-card UI state, and small reactive updates
- keep pure JavaScript for browser-native interactions that Datastar should not own

For external imported HTML widgets:

- keep the current generic iframe model
- allow pure JavaScript
- inject Datastar and the Lince bridge, but do not require them

## State Ownership

The web runtime should separate state into three groups.

### 1. Host-Owned Runtime State

Examples:

- `serverId`
- `viewId`
- stream enabled state
- auth lock state
- per-card persisted UI state

Recommended representation:

- bridge events
- Datastar signals

### 2. Server-Owned Data State

Examples:

- view rows
- table columns
- grouped record snapshots
- computed labels or counters derived from backend data

Recommended representation:

- HTML fragments by default
- signals only when the DOM shape is stable

### 3. Browser-Only Interaction State

Examples:

- drag and drop
- pointer gestures
- focus-sensitive editing
- canvas rendering
- terminal emulation

Recommended representation:

- pure JavaScript

## Default Transport Recommendation

For official widgets, the default stream profile should be:

- structure updates: HTML fragments
- small value and status updates: signal patches

If only one transport is chosen for a widget, choose HTML fragments unless the widget has a very stable DOM shape.

### Why HTML Fragments First

HTML fragments are the best default for capability because they handle:

- variable table schemas
- changing row counts
- empty, loading, locked, and error states
- grouped layouts
- server-owned repeated lists and cards
- server-controlled markup decisions

### Why Signals Still Matter

Signals are still important for:

- collapsed and expanded UI state
- selected tabs or modes
- filter text
- local sorting state
- status pills
- last-updated badges
- bridge-owned runtime state

## Sand Class Matrix

| Sand Class | Maud | Datastar | Pure JS | SSE HTML | SSE Signals | Best For |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Sand Class A: Fragment Shell | 80% | 15% | 5% | 90% | 10% | tables, lists, CRUD views, server-owned layouts |
| Sand Class B: Hybrid Reactive | 60% | 30% | 10% | 60% | 40% | kanban, dashboards, editable collections |
| Sand Class C: Signal Surface | 35% | 50% | 15% | 20% | 80% | clocks, status cards, compact dashboards, stable shells |
| Sand Class D: Imperative Specialist | 55% | 10% | 35% | 40% | 20% | drag and drop, terminal, canvas, media control |
| Sand Class E: External-Compatible Official | 45% | 20% | 35% | 30% | 20% | bridge-friendly widgets that still need to survive outside the official runtime |

The percentages are guidance, not rules. They describe where the center of gravity of the widget should be.

## Sand Class A: Fragment Shell

### Summary

Server-owned structure is rendered in Rust and patched into the widget as HTML fragments. Datastar is present but mostly used for small UI toggles and host state. JavaScript is minimal.

### Construction Pattern

- outer shell rendered with Maud
- inner content regions patched by server-rendered Maud fragments
- Datastar handles visibility, classes, and small local UI state
- JavaScript is limited to wiring that is hard to express declaratively

### Recommended SSE Style

- primary: HTML fragment patches
- secondary: signal patches for status and local UI mirrors

### Strengths

- highest capability
- easiest to keep correct when backend shape changes
- easiest to author in Rust
- easiest to keep business logic centralized

### Weaknesses

- can overpatch if the patch target is too large
- less ideal for high-frequency micro-updates

### Recommended Component Types

- generic view tables
- record browsers
- CRUD panels
- search result widgets
- file lists
- calendar agendas backed by backend data
- multi-state empty/loading/error/locked widgets

### Good Target Examples

- `view_table_editor`
- a future generic file explorer
- a future audit log widget

## Sand Class B: Hybrid Reactive

### Summary

The widget has a Maud-rendered shell and significant Datastar usage for interaction state, but server-owned lists and sections still arrive as HTML fragments. JavaScript stays focused on browser-native behavior.

### Construction Pattern

- Maud renders the stable shell
- Datastar owns UI signals and local state transitions
- server patches specific inner regions such as columns, rows, cards, or summaries
- JavaScript handles drag/drop, optimistic updates, or browser API edges

### Recommended SSE Style

- balanced HTML fragment patches and signal patches

### Strengths

- best general-purpose class
- keeps server data authoritative
- keeps local UI responsive
- adapts well to complex widgets with interactive chrome

### Weaknesses

- requires discipline about what belongs to server data versus local UI

### Recommended Component Types

- kanban boards
- dashboards with both summaries and changing lists
- widgets with collapsible lanes or sections
- widgets with per-card presentation modes
- widgets with optimistic row moves or inline edits

### Good Target Examples

- `kanban_record_view`
- future issue boards
- grouped operations dashboards

## Sand Class C: Signal Surface

### Summary

The DOM shape stays mostly fixed. Datastar signals drive most of the visible changes. Maud still provides the base shell. JavaScript is small and targeted.

### Construction Pattern

- Maud renders a stable shell
- Datastar signals drive content, classes, visibility, and styling
- server sends mostly signal patches
- HTML fragment patches are rare and limited

### Recommended SSE Style

- primary: signal patches
- secondary: occasional small HTML patches

### Strengths

- efficient when the DOM shape is stable
- easy to reason about for compact widgets
- good for rich local reactivity

### Weaknesses

- becomes awkward when repeated structures change often
- can push too much templating responsibility into browser expressions

### Recommended Component Types

- clocks
- counters
- weather summaries
- playback status surfaces
- compact health indicators
- small dashboard headers
- bridge and auth state monitors

### Good Target Examples

- `ops_clock`
- a compact server status widget
- a compact playback state widget

## Sand Class D: Imperative Specialist

### Summary

The widget still uses Maud for shell and some static structure, but browser interaction logic is too rich or too stateful to force into Datastar. JavaScript remains the main interaction engine.

### Construction Pattern

- Maud renders static structure and SSR fragments where useful
- Datastar is optional and kept small
- JavaScript owns the interaction loop

### Recommended SSE Style

- use HTML or signals only for the server-owned parts that are easy to separate
- avoid trying to express the whole widget as Datastar patches

### Strengths

- handles difficult browser APIs cleanly
- avoids awkward declarative code for inherently imperative behavior

### Weaknesses

- highest maintenance burden
- easiest place for duplicated transport logic to appear if discipline is weak

### Recommended Component Types

- terminal widgets
- canvas-based widgets
- complex drag/drop surfaces
- media transport controls with browser API integration
- image tools with zoom and pan

### Good Target Examples

- `local_terminal`
- canvas-heavy future widgets
- advanced bucket or image inspectors

## Sand Class E: External-Compatible Official

### Summary

This class is for official widgets that should still degrade gracefully if copied out of the full internal runtime. The widget can use the bridge and Datastar when present, but it keeps a stronger pure-JS fallback path than other official widgets.

### Construction Pattern

- Maud or static HTML shell
- Datastar used when available
- JavaScript fallback preserved intentionally

### Recommended SSE Style

- use internal official streams when embedded in Lince
- preserve a fallback contract for preview or external survival

### Strengths

- portable
- resilient in previews and less controlled environments

### Weaknesses

- more duplicated logic than the pure official runtime classes

### Recommended Component Types

- showcase widgets
- reference widgets
- minimal example widgets
- migration widgets during transition to the official stream system

### Good Target Examples

- `extra_simple`
- a reference bridge demo widget

## When To Send HTML Versus Signals

Use HTML fragments when:

- a list, table, lane, or repeated section changes
- the backend changes the shape of the data
- the server decides markup details
- you want Maud to remain the source of truth for structure

Use signal patches when:

- the DOM shape is fixed
- values, classes, or styles change
- the update is small and frequent
- the state belongs to local UI or host runtime state

Use both when:

- the shell is stable
- the content region is dynamic
- local UI preferences should survive server updates

This mixed mode should be treated as the standard design for most serious official widgets.

## Recommended Official Defaults

### Default Class For New Data Widgets

Use `Sand Class B: Hybrid Reactive`.

This should be the standard default for:

- backend view widgets
- grouped record widgets
- widgets with server data plus local presentation state

### Default Class For Generic Table And Collection Widgets

Use `Sand Class A: Fragment Shell`.

This should be the standard default for:

- generic tables
- search results
- collection views
- widgets where the backend controls the structure

### Default Class For Compact Status Widgets

Use `Sand Class C: Signal Surface`.

This should be the standard default for:

- metrics
- clocks
- compact summaries
- small indicators

### Default Class For Browser-Heavy Widgets

Use `Sand Class D: Imperative Specialist`.

This should be the standard default for:

- terminal
- canvas
- complex media and drag/drop interactions

## Current Widget Mapping

Recommended mapping for the current official set:

- `extra_simple`: Sand Class E during migration, then Sand Class A
- `view_table_editor`: Sand Class A
- `kanban_record_view`: Sand Class B
- `ops_clock`: Sand Class C
- `local_terminal`: Sand Class D
- `bucket_image_view`: Sand Class D or B depending on how much of the UI becomes server-owned
- `record_crud`: Sand Class A or B depending on whether the form stays mostly local or becomes more server-rendered

## Anti-Patterns

Avoid these as general policy for official widgets:

- parsing raw SSE blocks inside every widget
- rebuilding the entire widget with `innerHTML` on every snapshot
- forcing highly dynamic collections into signal-only rendering
- pushing bridge-owned runtime state into backend stream payloads
- using pure JavaScript as the default for official data widgets

## Proposed Internal Runtime Direction

The official runtime should move toward an instance-aware stream, for example:

- `/host/widgets/{instance_id}/stream`

That route should:

- resolve the card from board state
- read widget configuration and host-owned runtime state
- subscribe internally to local or remote backend data
- render Maud fragments for server-owned structure
- emit Datastar patch events and signal patch events

This allows:

- one official runtime contract for `sand/`
- continued support for generic imported HTML widgets
- less duplicated transport logic in widget code

## Final Rule

For official `sand/` widgets:

- default to Maud for structure
- default to HTML fragment SSE for server-owned data
- use Datastar signals for host and local UI state
- keep pure JavaScript only where the browser interaction model truly requires it

That is the best capability-first direction for the web version.
