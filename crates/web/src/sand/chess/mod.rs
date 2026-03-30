use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "chess-game.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "♞".into(),
            title: "Chess Game".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description:
                "Shared free-placement chess board that syncs a record_extension row through SSE."
                    .into(),
            details:
                "The widget reads a dedicated view, parses the chess state out of data_json as { history, current }, and writes moves back through the host table proxy. No Rust chess service or migration is required."
                    .into(),
            initial_width: 7,
            initial_height: 6,
            permissions: vec![
                "bridge_state".into(),
                "read_view_stream".into(),
                "write_records".into(),
                "write_table".into(),
            ],
        },
        head_links: vec![],
        inline_styles: vec![style()],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script())],
    }
}

fn body() -> Markup {
    html! {
        main id="app" class="app" data-lince-bridge-root {
            header class="hero panel" {
                div class="heroCopy" {
                    div class="eyebrow" { "Clown mode" }
                    h1 id="title" class="title" { "Chess Game" }
                    p id="copy" class="copy" {
                        "Tap a piece, then tap any square. Every piece can move anywhere, pawns promote on the far rank, and the move log is stored in the row."
                    }
                }
                div class="heroMeta" {
                    span id="status" class="status" data-tone="loading" { "Booting" }
                    button id="undo-button" class="button" type="button" { "Undo" }
                    button id="flip-button" class="button button--ghost" type="button" { "Flip Board" }
                }
            }

            section class="workspace" {
                section class="boardPanel panel" {
                    div class="panelHeader" {
                        div class="panelLead" {
                            div class="eyebrow" { "Shared board" }
                            h2 class="panelTitle" { "Board" }
                        }
                        div class="panelMeta" {
                            span id="source-pill" class="pill" { "Waiting for stream" }
                            span id="selection-pill" class="pill" { "No piece selected" }
                        }
                    }
                    div class="setupShell" {
                        p id="setup-copy" class="setupCopy" {
                            "Choose a server, then start a game or connect to an existing view."
                        }
                        div class="setupControls" {
                            input
                                id="view-id-input"
                                class="input"
                                type="text"
                                inputmode="numeric"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="View id";
                            button id="connect-button" class="button button--ghost" type="button" {
                                "Connect View"
                            }
                            button id="start-button" class="button" type="button" {
                                "Start New Game"
                            }
                        }
                    }
                    div class="boardWrap" {
                        div id="board" class="board" role="grid" aria-label="Chess board" {}
                    }
                }

                aside class="historyPanel panel" {
                    div class="panelHeader" {
                        div class="panelLead" {
                            div class="eyebrow" { "History" }
                            h2 class="panelTitle" { "Move log" }
                        }
                        span id="history-count" class="pill" { "0 moves" }
                    }
                    div id="history" class="historyList" {}
                }
            }
        }
    }
}

fn style() -> &'static str {
    r###"
      :root {
        color-scheme: dark;
        --bg: #080b10;
        --panel: #11151c;
        --panel-soft: #0d1117;
        --line: rgba(255, 255, 255, 0.09);
        --line-strong: rgba(255, 255, 255, 0.18);
        --text: #f5f1e8;
        --muted: #a7adba;
        --accent: #ff9f43;
        --accent-2: #67d7ff;
        --light: #ead7a2;
        --light-alt: #f2e4bf;
        --dark: #5d796e;
        --dark-alt: #395b72;
        --shadow: rgba(0, 0, 0, 0.33);
        --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        min-height: 100%;
        margin: 0;
        background: transparent;
      }

      body {
        min-height: 100vh;
        padding: 12px;
        color: var(--text);
        background:
          radial-gradient(circle at 12% 8%, rgba(255, 159, 67, 0.18), transparent 24%),
          radial-gradient(circle at 88% 14%, rgba(103, 215, 255, 0.14), transparent 22%),
          radial-gradient(circle at 50% 100%, rgba(255, 91, 159, 0.08), transparent 28%),
          linear-gradient(180deg, #0d1017, #06070b);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      button {
        font: inherit;
      }

      .app {
        min-height: calc(100vh - 24px);
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
        gap: 12px;
      }

      .panel {
        border: 1px solid var(--line);
        border-radius: 20px;
        background:
          linear-gradient(180deg, rgba(18, 23, 31, 0.98), rgba(13, 17, 23, 0.98));
        box-shadow:
          0 20px 44px var(--shadow),
          inset 0 1px 0 rgba(255, 255, 255, 0.02);
      }

      .hero {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 12px;
        padding: 14px 16px;
      }

      .heroCopy {
        min-width: 0;
      }

      .heroMeta {
        display: grid;
        gap: 8px;
        justify-items: end;
        flex: 0 0 auto;
      }

      .eyebrow {
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.67rem;
        font-weight: 600;
        letter-spacing: 0.18em;
        text-transform: uppercase;
      }

      .title {
        margin: 4px 0 0;
        font-size: 1.08rem;
        font-weight: 700;
        letter-spacing: -0.03em;
      }

      .copy {
        margin: 6px 0 0;
        max-width: 66ch;
        color: var(--muted);
        font-size: 0.82rem;
        line-height: 1.45;
      }

      .status,
      .pill,
      .button {
        display: inline-flex;
        align-items: center;
        gap: 7px;
        min-height: 32px;
        padding: 0 11px;
        border: 1px solid var(--line);
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.03);
        color: var(--muted);
        font-size: 0.72rem;
        line-height: 1;
        white-space: nowrap;
      }

      .status {
        letter-spacing: 0.06em;
        text-transform: uppercase;
      }

      .status[data-tone="live"] {
        color: #dbffe6;
        border-color: rgba(115, 240, 178, 0.24);
        background: rgba(24, 52, 36, 0.72);
      }

      .status[data-tone="loading"] {
        color: #f7e6bf;
        border-color: rgba(255, 159, 67, 0.24);
        background: rgba(55, 34, 14, 0.72);
      }

      .status[data-tone="error"] {
        color: #ffd8df;
        border-color: rgba(255, 127, 127, 0.24);
        background: rgba(64, 21, 31, 0.72);
      }

      .status[data-tone="warn"] {
        color: #fff1c7;
        border-color: rgba(255, 215, 102, 0.24);
        background: rgba(54, 42, 14, 0.72);
      }

      .status[data-tone="idle"] {
        color: var(--muted);
      }

      .button {
        color: var(--text);
        background: rgba(255, 255, 255, 0.05);
        cursor: pointer;
        transition:
          border-color 140ms ease,
          background 140ms ease,
          color 140ms ease,
          transform 140ms ease;
      }

      .button--ghost {
        color: var(--muted);
        background: rgba(255, 255, 255, 0.03);
      }

      .button:hover:not(:disabled) {
        border-color: var(--line-strong);
        background: rgba(255, 255, 255, 0.08);
        transform: translateY(-1px);
      }

      .button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      .setupShell {
        display: grid;
        gap: 10px;
        padding: 10px 14px 0;
      }

      .setupCopy {
        margin: 0;
        color: var(--muted);
        font-size: 0.78rem;
        line-height: 1.45;
      }

      .setupControls {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        align-items: center;
      }

      .input {
        min-width: 0;
        flex: 1 1 160px;
        min-height: 36px;
        padding: 0 12px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: rgba(255, 255, 255, 0.03);
        color: var(--text);
        font: inherit;
      }

      .input::placeholder {
        color: rgba(167, 173, 186, 0.86);
      }

      .input:focus {
        outline: none;
        border-color: rgba(103, 215, 255, 0.4);
        box-shadow: 0 0 0 3px rgba(103, 215, 255, 0.1);
      }

      .input:disabled {
        opacity: 0.55;
        cursor: not-allowed;
      }

      .workspace {
        min-height: 0;
        display: grid;
        grid-template-columns: minmax(0, 1.25fr) minmax(260px, 0.9fr);
        gap: 12px;
      }

      .boardPanel,
      .historyPanel {
        min-height: 0;
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
      }

      .panelHeader {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 10px;
        padding: 14px 14px 0;
      }

      .panelLead {
        min-width: 0;
      }

      .panelTitle {
        margin: 4px 0 0;
        font-size: 0.98rem;
        font-weight: 700;
        letter-spacing: -0.03em;
      }

      .panelMeta {
        display: flex;
        flex-wrap: wrap;
        justify-content: flex-end;
        gap: 8px;
        align-items: center;
      }

      .boardWrap {
        min-height: 0;
        display: grid;
        place-items: center;
        padding: 12px 14px 14px;
      }

      .board {
        width: min(100%, 72vmin);
        aspect-ratio: 1 / 1;
        display: grid;
        grid-template-columns: repeat(8, minmax(0, 1fr));
        grid-template-rows: repeat(8, minmax(0, 1fr));
        overflow: hidden;
        border-radius: 18px;
        border: 1px solid rgba(255, 255, 255, 0.08);
        box-shadow: 0 24px 56px rgba(0, 0, 0, 0.35);
      }

      .board[data-armed="true"] {
        box-shadow:
          0 0 0 1px rgba(103, 215, 255, 0.08),
          0 24px 56px rgba(0, 0, 0, 0.35);
      }

      .square {
        position: relative;
        display: grid;
        place-items: center;
        border: 0;
        padding: 0;
        margin: 0;
        color: inherit;
        cursor: pointer;
        transition:
          filter 140ms ease,
          transform 140ms ease,
          box-shadow 140ms ease;
      }

      .square--light {
        background: linear-gradient(180deg, var(--light-alt), var(--light));
      }

      .square--dark {
        background: linear-gradient(180deg, var(--dark-alt), var(--dark));
      }

      .square:hover {
        filter: brightness(1.06);
      }

      .square--armed {
        cursor: crosshair;
      }

      .square--selected {
        box-shadow:
          inset 0 0 0 3px rgba(255, 159, 67, 0.92),
          inset 0 0 0 5px rgba(0, 0, 0, 0.2);
      }

      .square--target {
        box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
      }

      .square__coord {
        position: absolute;
        left: 6px;
        bottom: 4px;
        color: rgba(255, 255, 255, 0.42);
        font-family: var(--mono);
        font-size: 0.57rem;
        letter-spacing: 0.08em;
        text-transform: uppercase;
        user-select: none;
      }

      .piece {
        position: relative;
        z-index: 1;
        display: grid;
        place-items: center;
        width: 74%;
        height: 74%;
        border-radius: 999px;
        font-size: clamp(1.05rem, 2.8vmin, 2.1rem);
        font-weight: 800;
        line-height: 1;
        user-select: none;
        transform: translateZ(0);
        transition:
          transform 140ms ease,
          box-shadow 140ms ease,
          filter 140ms ease;
      }

      .piece--white {
        color: #201c12;
        background:
          radial-gradient(circle at 30% 28%, #fff8e3, #e2d1a7 68%, #c6b386 100%);
        box-shadow:
          0 10px 16px rgba(0, 0, 0, 0.18),
          inset 0 0 0 1px rgba(255, 255, 255, 0.6);
      }

      .piece--black {
        color: #f7f9ff;
        background:
          radial-gradient(circle at 30% 28%, #3a4458, #171c27 68%, #0d1119 100%);
        box-shadow:
          0 10px 16px rgba(0, 0, 0, 0.28),
          inset 0 0 0 1px rgba(255, 255, 255, 0.12);
      }

      .piece--selected {
        transform: scale(1.08);
        filter:
          drop-shadow(0 0 8px rgba(255, 159, 67, 0.26))
          drop-shadow(0 0 18px rgba(255, 159, 67, 0.14));
      }

      .glyph,
      .queenGlyph {
        position: relative;
        display: grid;
        place-items: center;
        width: 100%;
        height: 100%;
      }

      .queenGlyph__plus,
      .queenGlyph__cross {
        position: absolute;
        inset: 0;
        display: grid;
        place-items: center;
      }

      .queenGlyph__plus {
        transform: scale(1.02);
      }

      .queenGlyph__cross {
        transform: scale(0.92);
        opacity: 0.95;
      }

      .historyList {
        min-height: 0;
        overflow: auto;
        padding: 12px 14px 14px;
        display: grid;
        gap: 8px;
        align-content: flex-start;
      }

      .historyEmpty {
        padding: 14px;
        border: 1px dashed var(--line);
        border-radius: 16px;
        color: var(--muted);
        font-size: 0.8rem;
        line-height: 1.45;
        background: rgba(255, 255, 255, 0.02);
      }

      .historyItem {
        display: grid;
        grid-template-columns: auto minmax(0, 1fr) auto;
        gap: 10px;
        align-items: center;
        padding: 10px 12px;
        border: 1px solid var(--line);
        border-radius: 16px;
        background: rgba(255, 255, 255, 0.03);
      }

      .historyItem--latest {
        border-color: rgba(255, 159, 67, 0.26);
        background: linear-gradient(
          180deg,
          rgba(255, 159, 67, 0.08),
          rgba(255, 255, 255, 0.03)
        );
      }

      .historyMini {
        width: 28px;
        height: 28px;
        border-radius: 999px;
        font-size: 0.72rem;
      }

      .historyCopy {
        min-width: 0;
      }

      .historyMove {
        font-size: 0.82rem;
        font-weight: 700;
        line-height: 1.35;
      }

      .historyMeta {
        margin-top: 2px;
        color: var(--muted);
        font-size: 0.72rem;
        line-height: 1.35;
      }

      .historyIndex {
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.66rem;
        letter-spacing: 0.12em;
        text-transform: uppercase;
      }

      @media (max-width: 920px) {
        .workspace {
          grid-template-columns: 1fr;
        }

        .board {
          width: min(100%, 84vmin);
        }
      }

      @media (max-width: 640px) {
        body {
          padding: 10px;
        }

        .hero,
        .panelHeader {
          padding-left: 12px;
          padding-right: 12px;
        }

        .boardWrap,
        .historyList {
          padding-left: 12px;
          padding-right: 12px;
          padding-bottom: 12px;
        }

        .hero {
          flex-direction: column;
        }

        .heroMeta,
        .panelMeta {
          justify-items: start;
          justify-content: flex-start;
        }
      }

      [hidden] {
        display: none !important;
      }
    "###
}

fn script() -> &'static str {
    r###"
      (() => {
        const app = document.getElementById("app");
        const boardEl = document.getElementById("board");
        const historyEl = document.getElementById("history");
        const statusEl = document.getElementById("status");
        const undoButton = document.getElementById("undo-button");
        const flipButton = document.getElementById("flip-button");
        const sourcePill = document.getElementById("source-pill");
        const selectionPill = document.getElementById("selection-pill");
        const historyCount = document.getElementById("history-count");
        const setupCopy = document.getElementById("setup-copy");
        const viewIdInput = document.getElementById("view-id-input");
        const connectButton = document.getElementById("connect-button");
        const startButton = document.getElementById("start-button");
        const frame = window.frameElement;
        const bridge = window.LinceWidgetHost || null;
        const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
        const storageKey = "chess-game/state/" + instanceId;

        const FILES = ["a", "b", "c", "d", "e", "f", "g", "h"];
        const PIECE_KINDS = new Set(["pawn", "rook", "knight", "bishop", "queen", "king"]);
        const PIECE_COLORS = new Set(["white", "black"]);
        const INITIAL_STATE = createInitialGame();

        let bridgeBound = false;
        let connectionGeneration = 0;
        let streamAbortController = null;
        let persistLock = false;
        let setupLock = false;
        let boardFlipped = false;
        let committedGame = cloneGame(INITIAL_STATE);
        let displayedGame = cloneGame(INITIAL_STATE);
        let selectedPieceId = null;
        let lastRowSignature = "";
        let currentConfig = {
          signature: "",
          mode: "local",
          serverId: "",
          viewId: null,
          rowId: null,
          sourceText: "Local preview",
          rowLabel: "",
          gameCode: "",
        };

        function isLocked() {
          return persistLock || setupLock;
        }

        function delay(ms) {
          return new Promise((resolve) => window.setTimeout(resolve, ms));
        }

        function cloneJsonValue(value, fallback = null) {
          try {
            if (value === undefined) {
              return fallback;
            }
            return JSON.parse(JSON.stringify(value));
          } catch {
            return fallback;
          }
        }

        function cloneGame(game) {
          return {
            history: Array.isArray(game?.history) ? cloneJsonValue(game.history, []) : [],
            current: {
              pieces: Array.isArray(game?.current?.pieces)
                ? cloneJsonValue(game.current.pieces, [])
                : [],
            },
          };
        }

        function escapeHtml(value) {
          return String(value)
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#39;");
        }

        function isPlainObject(value) {
          return Boolean(value) && Object.prototype.toString.call(value) === "[object Object]";
        }

        function parseInteger(value) {
          const parsed = Number.parseInt(String(value ?? ""), 10);
          return Number.isInteger(parsed) ? parsed : null;
        }

        function normalizeSquare(value) {
          const square = String(value || "").trim().toLowerCase();
          return /^[a-h][1-8]$/.test(square) ? square : "";
        }

        function normalizeColor(value, fallback = "white") {
          const color = String(value || fallback).trim().toLowerCase();
          return PIECE_COLORS.has(color) ? color : fallback;
        }

        function normalizeKind(value, fallback = "pawn") {
          const kind = String(value || fallback).trim().toLowerCase();
          return PIECE_KINDS.has(kind) ? kind : fallback;
        }

        function normalizePiece(rawPiece, fallbackId = "") {
          if (!isPlainObject(rawPiece)) {
            return null;
          }

          const square = normalizeSquare(rawPiece.square ?? rawPiece.position ?? rawPiece.at);
          if (!square) {
            return null;
          }

          const color = normalizeColor(rawPiece.color ?? rawPiece.side, "white");
          const kind = normalizeKind(rawPiece.kind ?? rawPiece.type, "pawn");
          const id =
            String(rawPiece.id || rawPiece.pieceId || `${color}-${kind}-${square}` || fallbackId)
              .trim() || `${color}-${kind}-${square}`;

          return { id, color, kind, square };
        }

        function normalizePieces(rawPieces) {
          const pieces = [];
          const byId = new Map();
          const bySquare = new Map();

          for (const rawPiece of Array.isArray(rawPieces) ? rawPieces : []) {
            const nextPiece = normalizePiece(rawPiece);
            if (!nextPiece) {
              continue;
            }

            const previousById = byId.get(nextPiece.id);
            if (previousById) {
              bySquare.delete(previousById.square);
            }

            const previousBySquare = bySquare.get(nextPiece.square);
            if (previousBySquare) {
              byId.delete(previousBySquare.id);
            }

            byId.set(nextPiece.id, nextPiece);
            bySquare.set(nextPiece.square, nextPiece);
          }

          for (const piece of byId.values()) {
            pieces.push(piece);
          }

          return pieces;
        }

        function normalizeCapturedPiece(rawPiece) {
          if (!isPlainObject(rawPiece)) {
            return null;
          }

          const square = normalizeSquare(rawPiece.square ?? rawPiece.position ?? rawPiece.at);
          const color = normalizeColor(rawPiece.color ?? rawPiece.side, "white");
          const kind = normalizeKind(rawPiece.kind ?? rawPiece.type, "pawn");
          const id =
            String(rawPiece.id || rawPiece.pieceId || `${color}-${kind}-${square}` || "")
              .trim() || `${color}-${kind}-${square}`;

          return square ? { id, color, kind, square } : null;
        }

        function normalizeHistoryEntry(rawEntry) {
          if (!isPlainObject(rawEntry)) {
            return null;
          }

          const from = normalizeSquare(rawEntry.from);
          const to = normalizeSquare(rawEntry.to);
          if (!from || !to) {
            return null;
          }

          const pieceColor = normalizeColor(rawEntry.pieceColor ?? rawEntry.color, "white");
          const pieceKindBefore = normalizeKind(rawEntry.pieceKindBefore ?? rawEntry.kindBefore ?? rawEntry.kind, "pawn");
          const pieceKindAfter = normalizeKind(rawEntry.pieceKindAfter ?? rawEntry.kindAfter ?? pieceKindBefore, pieceKindBefore);
          const captured = normalizeCapturedPiece(rawEntry.captured ?? rawEntry.capturedPiece);
          const createdAt = String(rawEntry.createdAt || rawEntry.at || new Date().toISOString());
          const pieceId =
            String(rawEntry.pieceId || rawEntry.id || `${pieceColor}-${pieceKindBefore}-${from}`)
              .trim() || `${pieceColor}-${pieceKindBefore}-${from}`;

          return {
            id: String(rawEntry.id || `${pieceId}-${createdAt}`).trim(),
            pieceId,
            pieceColor,
            pieceKindBefore,
            pieceKindAfter,
            from,
            to,
            captured,
            createdAt,
          };
        }

        function normalizeHistory(rawHistory) {
          const history = [];

          for (const rawEntry of Array.isArray(rawHistory) ? rawHistory : []) {
            const entry = normalizeHistoryEntry(rawEntry);
            if (entry) {
              history.push(entry);
            }
          }

          return history;
        }

        function createInitialPieces() {
          const pieces = [];

          const backRank = [
            ["rook", "a"],
            ["knight", "b"],
            ["bishop", "c"],
            ["queen", "d"],
            ["king", "e"],
            ["bishop", "f"],
            ["knight", "g"],
            ["rook", "h"],
          ];

          for (const [kind, file] of backRank) {
            pieces.push({ id: `white-${kind}-${file}1`, color: "white", kind, square: `${file}1` });
            pieces.push({ id: `black-${kind}-${file}8`, color: "black", kind, square: `${file}8` });
          }

          for (const file of FILES) {
            pieces.push({ id: `white-pawn-${file}2`, color: "white", kind: "pawn", square: `${file}2` });
            pieces.push({ id: `black-pawn-${file}7`, color: "black", kind: "pawn", square: `${file}7` });
          }

          return pieces;
        }

        function createInitialGame() {
          return {
            history: [],
            current: {
              pieces: createInitialPieces(),
            },
          };
        }

        function createGameCode() {
          if (window.crypto?.getRandomValues) {
            const buffer = new Uint32Array(1);
            window.crypto.getRandomValues(buffer);
            return String(buffer[0] % 100000000).padStart(8, "0");
          }

          return String(Math.floor(Math.random() * 100000000)).padStart(8, "0");
        }

        function buildChessGameName(code) {
          return `Chess Game ${String(code || "").trim() || createGameCode()}`;
        }

        function buildChessViewQuery(extensionId) {
          const safeExtensionId = Number(extensionId);
          return [
            "SELECT",
            "  re.id,",
            "  re.record_id,",
            "  re.namespace,",
            "  re.version,",
            "  re.data_json,",
            "  re.created_at,",
            "  re.updated_at,",
            "  r.head AS record_head",
            "FROM record_extension re",
            "JOIN record r ON r.id = re.record_id",
            `WHERE re.id = ${safeExtensionId}`,
          ].join(" ");
        }

        function readChessCardState(detail) {
          const meta = isPlainObject(detail?.meta) ? detail.meta : {};
          const cardState = isPlainObject(meta.cardState) ? meta.cardState : {};
          const chess = isPlainObject(cardState.chess) ? cardState.chess : {};

          return {
            viewId: parseInteger(chess.viewId),
            gameCode: String(chess.gameCode || "").trim(),
          };
        }

        function patchChessCardState(patch) {
          if (!bridge || typeof bridge.patchCardState !== "function") {
            return;
          }

          bridge.patchCardState({
            chess: patch,
          });
        }

        function parseGameEnvelope(rawEnvelope) {
          if (!isPlainObject(rawEnvelope)) {
            return { game: createInitialGame(), needsSeed: true };
          }

          const rawHistory = normalizeHistory(rawEnvelope.history);
          const rawCurrent = isPlainObject(rawEnvelope.current) ? rawEnvelope.current : {};
          const rawPieces = Array.isArray(rawCurrent.pieces)
            ? rawCurrent.pieces
            : isPlainObject(rawCurrent.board)
              ? Object.entries(rawCurrent.board).map(([square, piece]) => ({
                  ...(isPlainObject(piece) ? piece : {}),
                  square,
                }))
              : [];

          const pieces = normalizePieces(rawPieces);
          const needsSeed = pieces.length === 0;
          const game = needsSeed
            ? createInitialGame()
            : {
                history: rawHistory,
                current: {
                  pieces,
                },
              };

          return { game, needsSeed };
        }

        function parseRowPayload(row) {
          if (!isPlainObject(row)) {
            return null;
          }

          const rowId = parseInteger(row.id);
          const namespace = String(row.namespace || "").trim();
          const rawDataJson = String(row.data_json || "").trim();
          const rowLabel = namespace
            ? `row #${rowId ?? "?"} · ${namespace}`
            : `row #${rowId ?? "?"}`;

          let rawEnvelope = null;
          let needsSeed = false;

          if (rawDataJson && rawDataJson !== "NULL") {
            try {
              rawEnvelope = JSON.parse(rawDataJson);
            } catch {
              rawEnvelope = null;
              needsSeed = true;
            }
          } else {
            needsSeed = true;
          }

          const parsed = parseGameEnvelope(rawEnvelope);
          needsSeed = needsSeed || parsed.needsSeed;

          return {
            rowId,
            rowLabel,
            game: parsed.game,
            needsSeed,
          };
        }

        function pickChessRow(snapshot) {
          const rows = Array.isArray(snapshot?.rows) ? snapshot.rows : [];
          if (!rows.length) {
            return null;
          }

          if (currentConfig.rowId != null) {
            const matched = rows.find((row) => parseInteger(row?.id) === currentConfig.rowId);
            if (matched) {
              return matched;
            }
          }

          return rows[0];
        }

        function gameSignature(game) {
          return JSON.stringify({ history: game.history, current: game.current });
        }

        function pieceLabel(piece) {
          return `${piece.color} ${piece.kind}`;
        }

        function isBottomSidePiece(piece) {
          return boardFlipped ? piece.color === "black" : piece.color === "white";
        }

        function pieceKindGlyph(piece) {
          switch (piece.kind) {
            case "rook":
              return `<span class="glyph">+</span>`;
            case "bishop":
              return `<span class="glyph">x</span>`;
            case "queen":
              return `
                <span class="queenGlyph" aria-hidden="true">
                  <span class="queenGlyph__plus">+</span>
                  <span class="queenGlyph__cross">x</span>
                </span>
              `;
            case "king":
              return `<span class="glyph">◯</span>`;
            case "knight":
              return `<span class="glyph">λ</span>`;
            case "pawn":
              return `<span class="glyph">${isBottomSidePiece(piece) ? "v" : "^"}</span>`;
            default:
              return `<span class="glyph">?</span>`;
          }
        }

        function renderMiniPiece(piece) {
          return `
            <span class="piece piece--${escapeHtml(piece.color)} historyMini" aria-hidden="true">
              ${pieceKindGlyph(piece)}
            </span>
          `;
        }

        function renderPiece(piece, selected = false) {
          return `
            <span
              class="piece piece--${escapeHtml(piece.color)}${selected ? " piece--selected" : ""}"
              data-piece-id="${escapeHtml(piece.id)}"
              aria-hidden="true"
            >
              ${pieceKindGlyph(piece)}
            </span>
          `;
        }

        function getPieceAtSquare(game, square) {
          const normalizedSquare = normalizeSquare(square);
          if (!normalizedSquare) {
            return null;
          }

          return game.current.pieces.find((piece) => piece.square === normalizedSquare) || null;
        }

        function getPieceById(game, pieceId) {
          return game.current.pieces.find((piece) => piece.id === pieceId) || null;
        }

        function isPromotionSquare(piece, square) {
          const rank = Number.parseInt(String(square).slice(1), 10);
          if (!Number.isInteger(rank)) {
            return false;
          }

          return piece.kind === "pawn" && ((piece.color === "white" && rank === 8) || (piece.color === "black" && rank === 1));
        }

        function createMoveEntry(piece, from, to, nextKind, captured) {
          return {
            id: `move-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
            pieceId: piece.id,
            pieceColor: piece.color,
            pieceKindBefore: piece.kind,
            pieceKindAfter: nextKind,
            from,
            to,
            captured: captured ? cloneJsonValue(captured, null) : null,
            createdAt: new Date().toISOString(),
          };
        }

        function applyMove(game, pieceId, targetSquare) {
          const fromSquare = normalizeSquare(targetSquare);
          if (!fromSquare) {
            return null;
          }

          const movingPiece = getPieceById(game, pieceId);
          if (!movingPiece) {
            return null;
          }

          if (movingPiece.square === fromSquare) {
            return {
              game,
              selectedPieceId: null,
              move: null,
            };
          }

          const capturedPiece = getPieceAtSquare(game, fromSquare);
          const nextKind = isPromotionSquare(movingPiece, fromSquare) ? "queen" : movingPiece.kind;
          const nextPieces = [];

          for (const piece of game.current.pieces) {
            if (piece.id === movingPiece.id) {
              nextPieces.push({
                ...piece,
                square: fromSquare,
                kind: nextKind,
              });
              continue;
            }

            if (capturedPiece && piece.id === capturedPiece.id) {
              continue;
            }

            nextPieces.push(cloneJsonValue(piece, piece));
          }

          const move = createMoveEntry(movingPiece, movingPiece.square, fromSquare, nextKind, capturedPiece);
          return {
            game: {
              history: [...game.history, move],
              current: {
                pieces: nextPieces,
              },
            },
            selectedPieceId: null,
            move,
          };
        }

        function undoMove(game) {
          if (!game.history.length) {
            return null;
          }

          const lastMove = game.history[game.history.length - 1];
          const nextPieces = [];
          let restored = false;

          for (const piece of game.current.pieces) {
            if (piece.id === lastMove.pieceId) {
              nextPieces.push({
                ...piece,
                square: lastMove.from,
                kind: lastMove.pieceKindBefore,
              });
              restored = true;
              continue;
            }

            nextPieces.push(cloneJsonValue(piece, piece));
          }

          if (!restored) {
            nextPieces.push({
              id: lastMove.pieceId,
              color: lastMove.pieceColor,
              kind: lastMove.pieceKindBefore,
              square: lastMove.from,
            });
          }

          if (lastMove.captured) {
            nextPieces.push(cloneJsonValue(lastMove.captured, lastMove.captured));
          }

          return {
            game: {
              history: game.history.slice(0, -1),
              current: {
                pieces: normalizePieces(nextPieces),
              },
            },
            selectedPieceId: null,
          };
        }

        function setStatus(text, tone = "idle") {
          statusEl.textContent = text;
          statusEl.dataset.tone = tone;
        }

        function setSelectionPill(text) {
          selectionPill.textContent = text;
        }

        function setSourcePill(text) {
          sourcePill.textContent = text;
        }

        function setHistoryCount(count) {
          const label = count === 1 ? "1 move" : `${count} moves`;
          historyCount.textContent = label;
          undoButton.disabled = count === 0 || isLocked();
        }

        function syncBoardFlipButton() {
          if (!flipButton) {
            return;
          }

          flipButton.textContent = boardFlipped ? "White View" : "Black View";
          flipButton.setAttribute("aria-pressed", boardFlipped ? "true" : "false");
        }

        function syncSetupShell() {
          const hasServer = Boolean(String(currentConfig.serverId || "").trim());
          const activeViewId = parseInteger(currentConfig.viewId);
          const typedViewId = parseInteger(viewIdInput?.value ?? "");

          if (setupCopy) {
            if (!hasServer) {
              setupCopy.textContent =
                "Choose a server in the card config first. After that this widget can create a game or connect to an existing view.";
            } else if (activeViewId && currentConfig.rowId != null) {
              const label = currentConfig.gameCode
                ? `Chess Game ${currentConfig.gameCode}`
                : `view #${activeViewId}`;
              setupCopy.textContent =
                `${label} is connected. Start makes a fresh shared game, and Connect switches this widget to another view id.`;
            } else if (activeViewId) {
              setupCopy.textContent =
                `View #${activeViewId} is selected. The widget is connecting or ready to reconnect to that shared board.`;
            } else {
              setupCopy.textContent =
                "Start creates a record, an extension row, and a dedicated view. Connect attaches this widget to an existing shared view id.";
            }
          }

          if (viewIdInput && document.activeElement !== viewIdInput) {
            viewIdInput.value = activeViewId ? String(activeViewId) : "";
          }

          if (viewIdInput) {
            viewIdInput.disabled = isLocked() || !hasServer;
          }

          if (connectButton) {
            connectButton.disabled =
              isLocked() || !hasServer || typedViewId == null || typedViewId <= 0;
          }

          if (startButton) {
            startButton.disabled = isLocked() || !hasServer;
          }
        }

        function renderBoard() {
          const piecesBySquare = new Map();
          for (const piece of displayedGame.current.pieces) {
            piecesBySquare.set(piece.square, piece);
          }

          const selectedPiece = selectedPieceId ? getPieceById(displayedGame, selectedPieceId) : null;
          const armed = Boolean(selectedPiece);
          const squares = [];
          const fileOrder = boardFlipped ? [...FILES].reverse() : FILES;
          const rankOrder = boardFlipped ? [1, 2, 3, 4, 5, 6, 7, 8] : [8, 7, 6, 5, 4, 3, 2, 1];

          for (const rank of rankOrder) {
            for (const file of fileOrder) {
              const fileIndex = FILES.indexOf(file);
              const square = `${file}${rank}`;
              const piece = piecesBySquare.get(square) || null;
              const isSelectedSquare = Boolean(selectedPiece && selectedPiece.square === square);
              const classes = [
                "square",
                (fileIndex + rank) % 2 === 0 ? "square--light" : "square--dark",
              ];

              if (armed) {
                classes.push("square--armed");
                classes.push("square--target");
              }

              if (isSelectedSquare) {
                classes.push("square--selected");
              }

              squares.push(`
                <button
                  type="button"
                  class="${classes.join(" ")}"
                  data-square="${square}"
                  aria-label="${escapeHtml(`${square}${piece ? " " + pieceLabel(piece) : ""}`)}"
                  aria-pressed="${isSelectedSquare ? "true" : "false"}"
                >
                  ${piece ? renderPiece(piece, Boolean(selectedPiece && selectedPiece.id === piece.id)) : ""}
                  <span class="square__coord">${square}</span>
                </button>
              `);
            }
          }

          boardEl.dataset.armed = armed ? "true" : "false";
          boardEl.innerHTML = squares.join("");
        }

        function renderHistory() {
          if (!displayedGame.history.length) {
            historyEl.innerHTML = `
              <div class="historyEmpty">
                No moves yet. Tap a piece, then tap any square, and the move log will start growing here.
              </div>
            `;
            return;
          }

          const entries = [...displayedGame.history].reverse();
          historyEl.innerHTML = entries
            .map((entry, index) => {
              const capturedText = entry.captured
                ? `captures ${entry.captured.color} ${entry.captured.kind} on ${entry.captured.square}`
                : "open square";
              const promotionText =
                entry.pieceKindBefore !== entry.pieceKindAfter
                  ? `promotes to ${entry.pieceKindAfter}`
                  : "no promotion";
              const latestClass = index === 0 ? " historyItem--latest" : "";

              return `
                <article class="historyItem${latestClass}">
                  ${renderMiniPiece({
                    color: entry.pieceColor,
                    kind: entry.pieceKindAfter,
                  })}
                  <div class="historyCopy">
                    <div class="historyMove">
                      ${escapeHtml(`${entry.pieceColor} ${entry.pieceKindBefore} ${entry.from} → ${entry.to}`)}
                    </div>
                    <div class="historyMeta">
                      ${escapeHtml(`${capturedText} · ${promotionText}`)}
                    </div>
                  </div>
                  <span class="historyIndex">#${displayedGame.history.length - index}</span>
                </article>
              `;
            })
            .join("");
        }

        function renderShell() {
          const selectedPiece = selectedPieceId ? getPieceById(displayedGame, selectedPieceId) : null;
          const selectionText = selectedPiece
            ? `Selected: ${pieceLabel(selectedPiece)} at ${selectedPiece.square}`
            : "No piece selected";
          setSelectionPill(selectionText);
          setHistoryCount(displayedGame.history.length);
          setSourcePill(currentConfig.sourceText);
          syncBoardFlipButton();
          syncSetupShell();
          renderBoard();
          renderHistory();
        }

        function loadLocalGame() {
          persistLock = false;
          setupLock = false;
          currentConfig = {
            signature: `local:${instanceId}`,
            mode: "local",
            serverId: "",
            viewId: null,
            rowId: null,
            sourceText: "Local preview",
            rowLabel: "",
            gameCode: "",
          };

          try {
            const raw = window.localStorage.getItem(storageKey);
            if (raw) {
              const parsed = JSON.parse(raw);
              const normalized = parseGameEnvelope(parsed);
              committedGame = cloneGame(normalized.game);
              displayedGame = cloneGame(normalized.game);
              selectedPieceId = null;
              lastRowSignature = gameSignature(normalized.game);
              setStatus("Preview", "idle");
              renderShell();
              return;
            }
          } catch {
            // ignore preview storage failures
          }

          committedGame = cloneGame(INITIAL_STATE);
          displayedGame = cloneGame(INITIAL_STATE);
          selectedPieceId = null;
          lastRowSignature = gameSignature(INITIAL_STATE);
          setStatus("Preview", "idle");
          renderShell();
        }

        function flipBoardView() {
          boardFlipped = !boardFlipped;
          renderShell();
        }

        function enterProvisionableState(serverId, gameCode = "", tone = "idle", statusText = "") {
          persistLock = false;
          currentConfig = {
            signature: `${serverId}:`,
            mode: "setup",
            serverId,
            viewId: null,
            rowId: null,
            sourceText: gameCode ? `Chess Game ${gameCode}` : "No shared view connected",
            rowLabel: "",
            gameCode,
          };
          committedGame = cloneGame(INITIAL_STATE);
          displayedGame = cloneGame(INITIAL_STATE);
          selectedPieceId = null;
          lastRowSignature = gameSignature(INITIAL_STATE);
          setStatus(statusText || "Start or connect to a shared game.", tone);
          renderShell();
        }

        function persistLocalGame(game) {
          try {
            window.localStorage.setItem(storageKey, JSON.stringify({
              history: game.history,
              current: game.current,
            }));
          } catch {
            // ignore preview persistence failures
          }
        }

        async function createTableRow(serverId, table, object) {
          const response = await fetch(
            "/host/integrations/servers/" +
              encodeURIComponent(serverId) +
              "/table/" +
              encodeURIComponent(table),
            {
              method: "POST",
              headers: {
                "Content-Type": "application/json",
              },
              body: JSON.stringify(object),
            },
          );

          const payload = await response.json().catch(() => null);
          if (!response.ok) {
            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
            }
            throw new Error(payload?.error || `Falha ao criar ${table}.`);
          }

          const createdId = parseInteger(payload?.last_insert_rowid);
          if (createdId == null || createdId <= 0) {
            throw new Error(`O backend nao devolveu o id da nova row de ${table}.`);
          }

          return createdId;
        }

        async function persistSharedGame(game) {
          if (currentConfig.rowId == null) {
            throw new Error("Nao encontrei a row do chess game.");
          }

          const serverId = String(currentConfig.serverId || "").trim();
          const viewId = Number(currentConfig.viewId);
          if (!serverId || !Number.isInteger(viewId) || viewId <= 0) {
            throw new Error("O widget nao esta configurado com server_id e view_id validos.");
          }

          const response = await fetch(
            "/host/integrations/servers/" +
              encodeURIComponent(serverId) +
              "/table/record_extension/" +
              encodeURIComponent(String(currentConfig.rowId)),
            {
              method: "PATCH",
              headers: {
                "Content-Type": "application/json",
              },
              body: JSON.stringify({
                data_json: JSON.stringify({
                  history: game.history,
                  current: game.current,
                }),
              }),
            },
          );

          const payload = await response.json().catch(() => null);
          if (!response.ok) {
            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
            }
            throw new Error(payload?.error || "Falha ao persistir a row do chess.");
          }
        }

        function commitLocalGame(nextGame) {
          committedGame = cloneGame(nextGame);
          displayedGame = cloneGame(nextGame);
          selectedPieceId = null;
          persistLocalGame(nextGame);
          setStatus("Preview", "idle");
          renderShell();
        }

        async function commitSharedGame(nextGame, optimisticStatus = "Saving move...") {
          if (persistLock) {
            return;
          }

          persistLock = true;
          displayedGame = cloneGame(nextGame);
          selectedPieceId = null;
          renderShell();
          setStatus(optimisticStatus, "loading");

          try {
            await persistSharedGame(nextGame);
            committedGame = cloneGame(nextGame);
            displayedGame = cloneGame(nextGame);
            setStatus("Live", "live");
          } catch (error) {
            displayedGame = cloneGame(committedGame);
            selectedPieceId = null;
            setStatus(
              error instanceof Error ? error.message : "Falha ao salvar o lance.",
              "error",
            );
          } finally {
            persistLock = false;
            renderShell();
          }
        }

        function selectPiece(pieceId) {
          const piece = getPieceById(displayedGame, pieceId);
          if (!piece) {
            selectedPieceId = null;
            renderShell();
            return;
          }

          selectedPieceId = piece.id;
          renderShell();
        }

        function moveSelectedPiece(targetSquare) {
          if (isLocked()) {
            setStatus(
              setupLock ? "Finishing the current request..." : "Saving the last move...",
              "loading",
            );
            return;
          }

          if (currentConfig.mode === "setup") {
            setStatus("Start or connect to a shared game first.", "warn");
            return;
          }

          if (!selectedPieceId) {
            return;
          }

          const next = applyMove(displayedGame, selectedPieceId, targetSquare);
          if (!next) {
            selectedPieceId = null;
            renderShell();
            return;
          }

          if (next.move === null) {
            selectedPieceId = null;
            renderShell();
            return;
          }

          if (currentConfig.mode === "local") {
            commitLocalGame(next.game);
            return;
          }

          void commitSharedGame(next.game);
        }

        function undoLastMove() {
          if (isLocked() || !displayedGame.history.length) {
            return;
          }

          const next = undoMove(displayedGame);
          if (!next) {
            return;
          }

          if (currentConfig.mode === "local") {
            commitLocalGame(next.game);
            return;
          }

          void commitSharedGame(next.game, "Undoing last move...");
        }

        function readEventBlock(block) {
          const lines = String(block || "")
            .replace(/\r\n/g, "\n")
            .split("\n");
          let eventName = "message";
          const dataLines = [];

          for (const line of lines) {
            if (line.startsWith("event:")) {
              eventName = line.slice(6).trim();
              continue;
            }

            if (line.startsWith("data:")) {
              dataLines.push(line.slice(5).trimStart());
            }
          }

          return {
            event: eventName,
            data: dataLines.join("\n"),
          };
        }

        async function readEventStream(body, onEvent) {
          const reader = body.getReader();
          const decoder = new TextDecoder();
          let buffer = "";

          while (true) {
            const result = await reader.read();
            if (result.done) {
              break;
            }

            buffer += decoder.decode(result.value, { stream: true });
            buffer = buffer.replace(/\r\n/g, "\n");

            const blocks = buffer.split("\n\n");
            buffer = blocks.pop() || "";

            for (const block of blocks) {
              const trimmed = block.trim();
              if (!trimmed) {
                continue;
              }

              onEvent(readEventBlock(trimmed));
            }
          }

          if (buffer.trim()) {
            onEvent(readEventBlock(buffer));
          }
        }

        async function fetchSharedSnapshot(serverId, viewId) {
          const response = await fetch(
            "/host/integrations/servers/" +
              encodeURIComponent(serverId) +
              "/views/" +
              encodeURIComponent(viewId) +
              "/snapshot",
            {
              headers: {
                Accept: "application/json",
              },
            },
          );

          const payload = await response.json().catch(() => null);
          if (!response.ok) {
            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
            }
            throw new Error(payload?.error || "Nao foi possivel ler o snapshot da view.");
          }

          return payload;
        }

        function rememberConnectedView(viewId, gameCode = "") {
          patchChessCardState({
            viewId,
            gameCode: gameCode || null,
          });
        }

        async function connectAndRememberSharedGame(serverId, viewId, gameCode = "") {
          const connected = await connectSharedGame(serverId, viewId, gameCode);
          if (!connected) {
            return false;
          }

          rememberConnectedView(viewId, gameCode);
          return true;
        }

        async function startNewSharedGame() {
          const serverId = String(currentConfig.serverId || frame?.dataset?.linceServerId || "").trim();
          if (!serverId) {
            setStatus("Choose a server in the card config first.", "warn");
            renderShell();
            return;
          }

          if (setupLock) {
            return;
          }

          setupLock = true;
          setStatus("Creating a shared chess game...", "loading");
          renderShell();

          try {
            const gameCode = createGameCode();
            const gameName = buildChessGameName(gameCode);
            const recordId = await createTableRow(serverId, "record", {
              quantity: 0,
              head: gameName,
              body: "Shared chess board provisioned by the widget.",
            });
            const extensionId = await createTableRow(serverId, "record_extension", {
              record_id: recordId,
              namespace: "sand.chess",
              version: 1,
              data_json: JSON.stringify({
                history: INITIAL_STATE.history,
                current: INITIAL_STATE.current,
              }),
            });
            const viewId = await createTableRow(serverId, "view", {
              name: gameName,
              query: buildChessViewQuery(extensionId),
            });

            const connected = await connectAndRememberSharedGame(serverId, viewId, gameCode);
            if (!connected) {
              return;
            }
          } catch (error) {
            setStatus(
              error instanceof Error
                ? error.message
                : "Falha ao criar o chess compartilhado.",
              "error",
            );
          } finally {
            setupLock = false;
            renderShell();
          }
        }

        async function connectViewFromInput() {
          const serverId = String(currentConfig.serverId || frame?.dataset?.linceServerId || "").trim();
          if (!serverId) {
            setStatus("Choose a server in the card config first.", "warn");
            renderShell();
            return;
          }

          const viewId = parseInteger(viewIdInput?.value ?? "");
          if (viewId == null || viewId <= 0) {
            setStatus("Type a valid view id before connecting.", "warn");
            renderShell();
            return;
          }

          if (setupLock) {
            return;
          }

          setupLock = true;
          setStatus(`Connecting to view #${viewId}...`, "loading");
          renderShell();

          try {
            await connectAndRememberSharedGame(serverId, viewId);
          } finally {
            setupLock = false;
            renderShell();
          }
        }

        async function seedSharedGameIfNeeded(game, force = false) {
          if (!currentConfig.rowId) {
            return;
          }

          const signature = gameSignature(game);
          if (!force && signature === lastRowSignature) {
            return;
          }

          await persistSharedGame(game);
          lastRowSignature = signature;
        }

        function stopStream() {
          connectionGeneration += 1;
          if (streamAbortController) {
            streamAbortController.abort();
            streamAbortController = null;
          }
        }

        function applySnapshotPayload(snapshot) {
          const row = pickChessRow(snapshot);
          if (!row) {
            currentConfig.rowId = null;
            currentConfig.rowLabel = "No chess row returned";
            currentConfig.sourceText = currentConfig.viewId
              ? `View #${currentConfig.viewId} returned no chess row`
              : "No row returned by the configured view";
            committedGame = cloneGame(INITIAL_STATE);
            displayedGame = cloneGame(INITIAL_STATE);
            selectedPieceId = null;
            setStatus("Missing row", "warn");
            renderShell();
            return { needsSeed: false, foundRow: false };
          }

          const parsedRow = parseRowPayload(row);
          if (!parsedRow) {
            setStatus("Invalid chess row", "error");
            return { needsSeed: false, foundRow: false };
          }

          currentConfig.rowId = parsedRow.rowId;
          currentConfig.rowLabel = parsedRow.rowLabel;
          currentConfig.sourceText = currentConfig.viewId
            ? `view #${currentConfig.viewId} · ${parsedRow.rowLabel}`
            : `${parsedRow.rowLabel} · shared`;
          committedGame = cloneGame(parsedRow.game);
          displayedGame = cloneGame(parsedRow.game);
          selectedPieceId = null;
          lastRowSignature = gameSignature(parsedRow.game);
          renderShell();
          return {
            needsSeed: parsedRow.needsSeed,
            foundRow: true,
          };
        }

        async function connectSharedGame(serverId, viewId, gameCode = "") {
          const generation = ++connectionGeneration;
          currentConfig = {
            signature: `${serverId}:${viewId}`,
            mode: "shared",
            serverId,
            viewId,
            rowId: null,
            sourceText: `Loading view #${viewId}...`,
            rowLabel: "",
            gameCode: gameCode || currentConfig.gameCode || "",
          };
          persistLock = false;
          setStatus("Loading shared board...", "loading");
          setSourcePill(`Loading view #${viewId}...`);
          renderShell();

          let needsSeed = false;
          try {
            const snapshot = await fetchSharedSnapshot(serverId, viewId);
            if (generation !== connectionGeneration) {
              return false;
            }

            const result = applySnapshotPayload(snapshot);
            needsSeed = result.needsSeed;
            if (!result.foundRow) {
              enterProvisionableState(
                serverId,
                gameCode || currentConfig.gameCode,
                "warn",
                `View #${viewId} did not return a chess row.`,
              );
              return false;
            }

            setStatus(needsSeed ? "Seeding row..." : "Live", needsSeed ? "loading" : "live");
            if (needsSeed) {
              persistLock = true;
              renderShell();
              try {
                await seedSharedGameIfNeeded(displayedGame, true);
              } finally {
                persistLock = false;
                renderShell();
              }
            }

            if (generation !== connectionGeneration) {
              return false;
            }

            setStatus("Live", "live");
            void streamSharedGame(serverId, viewId, generation);
            return true;
          } catch (error) {
            if (generation !== connectionGeneration) {
              return false;
            }

            enterProvisionableState(
              serverId,
              gameCode || currentConfig.gameCode,
              "error",
              error instanceof Error
                ? error.message
                : "Nao consegui abrir o chess compartilhado.",
            );
            return false;
          }
        }

        async function streamSharedGame(serverId, viewId, generation) {
          const controller = new AbortController();
          streamAbortController = controller;
          const url =
            "/host/integrations/servers/" +
            encodeURIComponent(serverId) +
            "/views/" +
            encodeURIComponent(viewId) +
            "/stream";

          while (generation === connectionGeneration) {
            try {
              const response = await fetch(url, {
                headers: {
                  Accept: "text/event-stream",
                },
                signal: controller.signal,
              });

              if (generation !== connectionGeneration) {
                return;
              }

              if (response.status === 401) {
                window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
                setStatus("Server auth expired", "error");
                return;
              }

              if (!response.ok || !response.body) {
                const payload = await response.text().catch(() => "");
                throw new Error(payload || "Nao foi possivel abrir o stream da view.");
              }

              setStatus("Live", "live");
              await readEventStream(response.body, (event) => {
                if (generation !== connectionGeneration) {
                  return;
                }

                if (event.event === "snapshot") {
                  try {
                    const payload = JSON.parse(event.data);
                    applySnapshotPayload(payload);
                  } catch (error) {
                    setStatus("Snapshot parse failed", "error");
                  }
                  return;
                }

                if (event.event === "error") {
                  try {
                    const payload = JSON.parse(event.data);
                    setStatus(payload?.error || "Stream error", "error");
                  } catch {
                    setStatus(event.data || "Stream error", "error");
                  }
                }
              });

              if (generation !== connectionGeneration) {
                return;
              }

              setStatus("Reconnecting...", "loading");
              await delay(1000);
            } catch (error) {
              if (controller.signal.aborted || generation !== connectionGeneration) {
                return;
              }

              setStatus(
                error instanceof Error ? error.message : "Falha ao ler o stream.",
                "warn",
              );
              await delay(1200);
            }
          }
        }

        function applyBridgeDetail(detail) {
          const meta = isPlainObject(detail?.meta) ? detail.meta : {};
          const chessState = readChessCardState(detail);
          const serverId = String(meta.serverId || frame?.dataset?.linceServerId || "").trim();
          const viewId = parseInteger(
            meta.viewId ?? chessState.viewId ?? frame?.dataset?.linceViewId ?? "",
          );
          const signature = `${serverId}:${viewId ?? ""}`;
          if (signature === currentConfig.signature) {
            return;
          }

          stopStream();

          if (serverId && viewId) {
            void connectSharedGame(serverId, viewId, chessState.gameCode);
            return;
          }

          if (serverId) {
            enterProvisionableState(serverId, chessState.gameCode);
            return;
          }

          loadLocalGame();
        }

        function bindBridgeWhenReady() {
          if (bridgeBound || !bridge || typeof bridge.subscribe !== "function") {
            return false;
          }

          bridgeBound = true;
          bridge.subscribe((detail) => {
            applyBridgeDetail(detail);
          });
          bridge.requestState?.();
          return true;
        }

        function handleBoardClick(event) {
          const squareButton = event.target.closest("[data-square]");
          if (!squareButton || !boardEl.contains(squareButton)) {
            return;
          }

          const square = String(squareButton.dataset.square || "").trim();
          const pieceButton = event.target.closest("[data-piece-id]");
          const pieceId = pieceButton ? String(pieceButton.dataset.pieceId || "").trim() : "";

          if (isLocked()) {
            setStatus(
              setupLock ? "Finishing the current request..." : "Saving the last move...",
              "loading",
            );
            return;
          }

          if (currentConfig.mode === "setup") {
            setStatus("Start or connect to a shared game first.", "warn");
            return;
          }

          if (selectedPieceId) {
            moveSelectedPiece(square);
            return;
          }

          if (pieceId) {
            selectPiece(pieceId);
          }
        }

        undoButton.addEventListener("click", () => {
          undoLastMove();
        });

        flipButton.addEventListener("click", () => {
          flipBoardView();
        });

        startButton.addEventListener("click", () => {
          void startNewSharedGame();
        });

        connectButton.addEventListener("click", () => {
          void connectViewFromInput();
        });

        viewIdInput.addEventListener("input", () => {
          syncSetupShell();
        });

        viewIdInput.addEventListener("keydown", (event) => {
          if (event.key !== "Enter") {
            return;
          }

          event.preventDefault();
          void connectViewFromInput();
        });

        boardEl.addEventListener("click", handleBoardClick);
        app.addEventListener("lince-bridge-state", (event) => {
          applyBridgeDetail(event.detail);
        });
        window.addEventListener("beforeunload", () => {
          stopStream();
        });

        displayedGame = cloneGame(INITIAL_STATE);
        committedGame = cloneGame(INITIAL_STATE);
        renderShell();
        setStatus("Booting", "loading");

        if (!bindBridgeWhenReady()) {
          window.setTimeout(bindBridgeWhenReady, 0);
        }

        applyBridgeDetail({
          meta: {
            serverId: frame?.dataset?.linceServerId || "",
            viewId: frame?.dataset?.linceViewId || "",
          },
        });
      })();
    "###
}
