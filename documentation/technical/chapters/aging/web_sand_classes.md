# Web Sand Classes Technical Specification

## Purpose

This document defines the official class taxonomy for `sand/` widgets in the Lince web runtime.

The purpose of the taxonomy is:

- to save explanation time
- to make architecture discussions faster
- to document how a widget usually deals with data, state, transport, and runtime
- to provide a shared cookbook vocabulary such as `Engineer`, `Clown`, or `Mercenary`

These classes are documentation and clustering. They are not hard constraints.

## Scope

This specification applies primarily to official widgets built inside `crates/web/src/sand/`.

The same language may also be used when discussing:

- imported HTML widgets
- embedded third-party surfaces
- framework-backed integrations
- browser-engine or WASM-heavy widgets

## Classification principles

The classes are defined from five questions:

1. Where does truth normally live?
2. How does truth normally reach the widget?
3. Who normally shapes the UI?
4. Where does interaction state normally live?
5. How portable or foreign is the runtime?

The classes are descriptive, not prescriptive.

A widget may honestly be described as:

- `Engineer`
- `Clown with Monk traits`

Or other ways. That is expected.

## Core actors

### Backend

The Lince domain side.

It normally owns:

- persisted business data
- table CRUD
- view queries
- semantic actions
- official SSE streams

### Host

The Lince web runtime that embeds the widget.

It normally owns:

- widget instance identity
- permissions
- auth state
- board layout
- persisted `widgetState`
- stream enable or pause state

### Bridge

The communication layer from host to widget.

It should mainly carry:

- bootstrap config
- runtime events
- auth and permission state
- widget state patching
- host actions

It should not be treated as the main bulk transport for large server-owned collections.

### External system

Anything outside Lince, including:

- websites
- APIs
- apps
- local files
- foreign frameworks
- browser/device APIs
- WebAssembly runtimes

## Resource definitions

The matrices below use the following resources.

### Maud templating

Rust-side HTML construction used for:

- widget shells
- repeated server-owned structure
- forms
- empty/loading/error/auth states

### Datastar signals in frontend

Browser-local Datastar signal state used for:

- view mode
- collapse state
- draft UI state
- badges and small status changes
- anything else the user may want to construct in interactivity, look and feel.

### Datastar signals from backend

Signal patches emitted by the backend or host runtime, usually over SSE.

Use this for:

- small value changes
- runtime status
- connection/auth indicators
- stable-shell reactive updates

### Datastar HTML fragments from backend

HTML fragment patches emitted by the backend or host runtime, usually over SSE.

Use this for:

- repeated collections
- dynamic tables
- cards and lanes
- server-owned structural changes

### Raw JSON or hand-parsed data

The widget receives raw data and is responsible for parsing and shaping UI itself.

This includes:

- raw JSON SSE
- JSON fetched over HTTP
- client-side transformation and rendering

### Host or bridge runtime state

Runtime data that is not business truth, such as:

- `serverId`
- `viewId`
- auth state
- stream state
- `widgetState`

### Official Lince endpoints

Lince-owned routes and contracts, such as:

- official widget streams
- generic table routes
- view routes
- semantic action endpoints

### External APIs, pages, or apps

Foreign systems outside Lince, such as:

- third-party APIs
- embedded websites
- wrapped apps
- imported HTML

### Files or local resources

Non-backend resources such as:

- local file imports
- browser file handles
- media blobs
- local device or browser resources

### Iframe or embed isolation

Isolation-oriented embedding where the widget intentionally hosts or wraps a foreign surface.

### Foreign framework runtime

A vendored or mounted runtime such as:

- React
- Vue
- Svelte
- Web Components
- another JS UI runtime

### WASM or browser-engine runtime

A non-trivial engine-driven runtime such as:

- terminal engine
- canvas engine
- WebGL scene
- WASM-powered logic or rendering core

## Scale legend

All resource scales use:

- `none`
- `low`
- `mid`
- `high`

These values express probability and amount of usage, not permission.

## Resource matrix

This matrix answers: what does this class usually lean on?

| Resource                             | Engineer | Clown | Monk | Mercenary | Astromancer |
| ------------------------------------ | -------- | ----- | ---- | --------- | ----------- |
| Maud templating                      | high     | low   | none | none      | none        |
| Datastar signals in frontend         | low      | high  | low  | none      | none        |
| Datastar signals from backend        | low      | high  | none | none      | none        |
| Datastar HTML fragments from backend | high     | low   | none | none      | none        |
| Raw JSON or hand-parsed data         | none     | none  | high | low       | low         |
| Host or bridge runtime state         | high     | high  | low  | low       | low         |
| Official Lince endpoints             | high     | mid   | low  | none      | low         |
| External APIs, pages, or apps        | none     | low   | low  | high      | none        |
| Files or local resources             | none     | low   | low  | mid       | mid         |
| Iframe or embed isolation            | none     | none  | none | mid       | none        |
| Foreign framework runtime            | none     | none  | mid  | mid       | none        |
| WASM or browser-engine runtime       | none     | none  | none | none      | high        |

## Interpretation notes

### Engineer

- Engineer is fragment-first, not signal-first
- Engineer still allows a little imperative JS for browser behavior
- Engineer is strongly tied to official Lince contracts

### Clown

- Clown is signal-first, but not signal-only
- small backend fragment use is still normal when a stable shell needs selected structural patches

### Monk

- Monk is not simply only using "lots of JS"
- Monk specifically means being a self sufficient component, may call external data and deal with it by hand.
- Client-shaped rendering from raw data contracts
- vendored framework use is common enough to document explicitly

### Mercenary

- Used for integrating a lot with other API's, services, embedding other pages and resources.
- Mercenary is defined by foreign negotiation
- it may use bridge or Lince contracts opportunistically, but those are not its center of gravity

### Astromancer

- Astromancer is runtime-heavy, not integration-heavy
- The use case of WASM fits perfectly in Astromancer, it is for esoteric runtimes.
- the engine shapes the interactive surface
- resources are inputs, not the UI owner

## Five-question matrix

This matrix answers how each class normally works.

| Question                                    | Engineer                                               | Clown                                                                                          | Monk                                                                   | Mercenary                                                   | Astromancer                                                     |
| ------------------------------------------- | ------------------------------------------------------ | ---------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------------------- |
| Where does truth normally live?             | Backend                                                | Mixed: backend for business, frontend for presentation                                         | Varied, often client-owned or remote-contract-owned                    | Foreign system or local widget space, with Lince optional   | Resource or engine dependent                                    |
| How does truth normally reach the widget?   | Official backend stream and endpoints through the host | Backend seed, optional backend stream, host runtime state, and heavy signal-driven local state | Raw JSON SSE, fetch JSON, manual parsing, or vendored runtime adapters | Foreign page, foreign API, files, embeds, or adapter layers | File, buffer, resource binding, engine feed, or runtime adapter |
| Who normally shapes the UI?                 | Maud plus backend fragments and a few signals          | Small shell plus frontend Datastar signals                                                     | JS or vendored framework                                               | Foreign UI, foreign framework, or adapter runtime           | JS or WASM engine, with DOM shell secondary                     |
| Where does interaction state normally live? | Frontend for ergonomics, backend for domain mutations  | Mostly frontend                                                                                | Varied, usually frontend                                               | Mostly frontend or foreign runtime                          | Engine-local or use-case dependent                              |
| How portable or foreign is the runtime?     | Low, strongly Lince-coupled                            | Low to mid, still fairly Lince-shaped                                                          | Mid to high, usually self-contained                                    | High, intentionally foreign-capable                         | Mid, more constrained by runtime capabilities than by Lince     |

## Class profiles

## Engineer

### Definition

Engineer is the default official class for robust Lince-native workflows.

### Truth and transport

- truth normally lives in the backend
- truth normally reaches the widget through official streams and endpoints
- host and bridge state are significant, but remain runtime control-plane data

### UI shaping

- Maud is the main structural tool
- Datastar HTML fragments are the main update mechanism for server-owned structure
- Datastar backend signals are secondary and support stable-shell updates

### JavaScript profile

Use JavaScript sparingly for:

- drag and drop edges
- resize behavior
- focus-sensitive editing
- browser APIs that should not be forced into Datastar

### API profile

- official Lince endpoints: high
- external APIs: none
- iframe/embed isolation: none

### WASM profile

- none

### Recommended fits

- record tables
- CRUD panels
- search results
- record browsers
- audit logs
- server-owned dashboards

## Clown

### Definition

Clown is the signal-heavy, frontend-reactive class.

### Truth and transport

- truth is often mixed
- backend may still own business data
- frontend often owns a large share of presentation and interaction state

### UI shaping

- the shell is small and stable
- Datastar signals do most of the visible orchestration
- small backend fragment use is still acceptable when selected structural patches help

### JavaScript profile

Use JavaScript lightly for:

- imperative browser edges
- local helpers around Datastar-driven state
- interactions that are awkward in pure signals

### API profile

- official Lince endpoints: mid
- external APIs: low
- iframe/embed isolation: none

### WASM profile

- none

### Recommended fits

- clocks
- counters
- compact status widgets
- inspectors with several local display modes
- dashboards with stable shells and lively local state

## Monk

### Definition

Monk is the client-shaped class built from raw data contracts.

### Truth and transport

- truth is varied
- the widget often consumes raw JSON SSE or fetched JSON
- Lince may still be the source, but the browser takes responsibility for shaping the result

### UI shaping

- Maud is absent or minimal
- JS or a vendored runtime shapes the UI
- Datastar is optional and minor

### JavaScript profile

Use JavaScript as the primary rendering tool for:

- parsing raw snapshots
- building DOM
- wiring library lifecycles
- owning client-shaped interaction flows

### API profile

- official Lince endpoints: low
- external APIs: low
- foreign framework runtime: mid
- iframe/embed isolation: none

### WASM profile

- none

### Recommended fits

- experimental widgets
- client-heavy graphs
- legacy ports
- widgets using libraries that expect raw data

## Mercenary

### Definition

Mercenary is the integration-first, foreign-capable class.

### Truth and transport

- truth often lives outside Lince
- the widget may wrap, embed, fetch from, or adapt foreign systems
- Lince may participate, but is not the sole world the widget serves

### UI shaping

- a foreign page, foreign framework, or adapter runtime often shapes the UI
- Lince bridge or host support is opportunistic rather than central

### JavaScript profile

Use JavaScript mainly for:

- adapters
- wrappers
- embed coordination
- framework mounting
- API mediation

### API profile

- official Lince endpoints: none
- external APIs, pages, or apps: high
- iframe/embed isolation: mid
- foreign framework runtime: mid

### WASM profile

- none

### Recommended fits

- embeds
- foreign sites or apps
- wrapper widgets
- migration widgets
- framework-backed integrations
- imported HTML with fallback behavior

## Astromancer

### Definition

Astromancer is the runtime-heavy class for engine-driven browser surfaces.

### Truth and transport

- truth depends on the engine and resource model
- data may come from files, buffers, resources, streams, or runtime bindings

### UI shaping

- the engine shapes the meaningful interactive surface
- the DOM shell exists mainly for chrome, controls, and fallback states

### JavaScript profile

Use JavaScript for:

- engine integration
- lifecycle orchestration
- browser capability access
- glue around WASM or other runtime cores

### API profile

- official Lince endpoints: low
- external APIs: none
- iframe/embed isolation: none

### WASM profile

- high

### Recommended fits

- terminal widgets
- canvas surfaces
- media tools
- graph engines
- WASM-powered inspectors

## Mixing guidance

These classes are most useful when mixed honestly.

Examples:

- an official Kanban is `Engineer with Clown traits`
- a portable integration can be `Mercenary with Monk fallback`
- a terminal can be `Astromancer with Engineer chrome`

## Practical defaults

Use `Engineer` when:

- the backend should remain the main authority
- structure is important
- the widget is official and long-lived

Use `Clown` when:

- the shell is stable
- local presentation behavior matters a lot
- Datastar signals are doing the visible work

Use `Monk` when:

- the widget wants raw data and client-owned rendering on purpose

Use `Mercenary` when:

- the widget must negotiate with outside systems or foreign runtimes

Use `Astromancer` when:

- the browser runtime itself is the product
