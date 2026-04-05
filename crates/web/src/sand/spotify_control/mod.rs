use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, PreEscaped, html};

pub(crate) const FEATURE_FLAG: &str = "sand.spotify_control";

const STYLE: &str = r#"
      :root {
        color-scheme: dark;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        --bg: #12161b;
        --bg-soft: #171c23;
        --bg-muted: #1c222b;
        --line: rgba(255, 255, 255, 0.08);
        --line-strong: rgba(255, 255, 255, 0.12);
        --text: #eef2f6;
        --text-soft: #c6cdd7;
        --text-muted: #87909b;
        --accent: #95c3a0;
        --accent-ink: #0a0d0b;
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        min-height: 100%;
      }

      body {
        margin: 0;
        display: grid;
        min-height: 100vh;
        padding: 16px;
        background: var(--bg);
        color: var(--text);
      }

      button {
        font: inherit;
      }

      .screen {
        display: grid;
        align-content: start;
        gap: 14px;
        min-height: 100%;
      }

      .eyebrow {
        color: var(--text-muted);
        font-size: 10px;
        font-weight: 600;
        letter-spacing: 0.16em;
        text-transform: uppercase;
      }

      .setup-header,
      .player-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 12px;
      }

      .title {
        margin: 6px 0 0;
        font-size: 17px;
        font-weight: 600;
        letter-spacing: -0.03em;
      }

      .copy {
        margin: 6px 0 0;
        color: var(--text-soft);
        font-size: 13px;
        line-height: 1.5;
      }

      .pill {
        display: inline-flex;
        align-items: center;
        min-height: 30px;
        padding: 0 10px;
        border: 1px solid var(--line);
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.03);
        color: var(--text-soft);
        font-size: 10px;
        font-weight: 600;
        letter-spacing: 0.12em;
        text-transform: uppercase;
        white-space: nowrap;
      }

      .accounts {
        display: grid;
        gap: 8px;
      }

      .account-option {
        display: grid;
        grid-template-columns: auto minmax(0, 1fr) auto;
        gap: 12px;
        align-items: center;
        width: 100%;
        padding: 12px;
        border: 1px solid var(--line);
        border-radius: 18px;
        background: var(--bg-soft);
        color: inherit;
        text-align: left;
        cursor: pointer;
        transition:
          border-color 160ms cubic-bezier(0.22, 1, 0.36, 1),
          background 160ms cubic-bezier(0.22, 1, 0.36, 1),
          transform 160ms cubic-bezier(0.22, 1, 0.36, 1);
      }

      .account-option.is-selected {
        border-color: rgba(149, 195, 160, 0.34);
        background: #181f1c;
      }

      .account-option__avatar {
        display: grid;
        place-items: center;
        width: 34px;
        height: 34px;
        border-radius: 12px;
        background: var(--bg-muted);
        color: var(--text);
        font-size: 12px;
        font-weight: 700;
      }

      .account-option__name {
        display: block;
        color: var(--text);
        font-size: 13px;
        font-weight: 600;
      }

      .account-option__meta {
        display: block;
        color: var(--text-muted);
        font-size: 12px;
      }

      .account-option__check {
        width: 18px;
        height: 18px;
        border: 1px solid var(--line);
        border-radius: 999px;
        background: #0f1317;
      }

      .account-option.is-selected .account-option__check {
        border-color: rgba(149, 195, 160, 0.38);
        background: var(--accent);
        box-shadow: inset 0 0 0 4px #12161b;
      }

      .setup-note {
        padding: 12px;
        border: 1px solid var(--line);
        border-radius: 18px;
        background: rgba(255, 255, 255, 0.02);
        color: var(--text-muted);
        font-size: 12px;
        line-height: 1.55;
      }

      .connect-button,
      .icon-button {
        border: 1px solid var(--line);
        transition:
          border-color 160ms cubic-bezier(0.22, 1, 0.36, 1),
          background 160ms cubic-bezier(0.22, 1, 0.36, 1),
          color 160ms cubic-bezier(0.22, 1, 0.36, 1),
          transform 160ms cubic-bezier(0.22, 1, 0.36, 1);
      }

      .connect-button {
        min-height: 44px;
        padding: 0 14px;
        border-radius: 16px;
        background: var(--accent);
        color: var(--accent-ink);
        font-size: 13px;
        font-weight: 600;
        cursor: pointer;
      }

      .connect-button:hover,
      .icon-button:hover,
      .transport__button:hover,
      .meta-button:hover {
        transform: translateY(-1px);
      }

      .player {
        grid-template-rows: auto auto 1fr auto auto;
      }

      .player-cover {
        position: relative;
        min-height: 0;
        overflow: hidden;
        border: 1px solid var(--line-strong);
        border-radius: 24px;
        background: #191e26;
      }

      .player-cover svg {
        display: block;
        width: 100%;
        height: auto;
        aspect-ratio: 1 / 1;
      }

      .player-info {
        display: grid;
        gap: 5px;
      }

      .track-title {
        margin: 0;
        font-size: 16px;
        font-weight: 600;
        letter-spacing: -0.03em;
      }

      .track-artist {
        margin: 0;
        color: var(--text-soft);
        font-size: 13px;
      }

      .progress {
        display: grid;
        gap: 8px;
      }

      .progress__bar {
        height: 5px;
        overflow: hidden;
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.08);
      }

      .progress__fill {
        width: 100%;
        height: 100%;
        border-radius: inherit;
        background: var(--text);
        transform: scaleX(0);
        transform-origin: left center;
      }

      .progress__meta {
        display: flex;
        justify-content: space-between;
        gap: 12px;
        color: var(--text-muted);
        font-size: 11px;
      }

      .transport {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, auto));
        justify-content: center;
        gap: 10px;
      }

      .transport__button,
      .meta-button,
      .icon-button {
        display: inline-grid;
        place-items: center;
        min-width: 38px;
        height: 38px;
        padding: 0 12px;
        border-radius: 14px;
        background: var(--bg-soft);
        color: var(--text-soft);
        cursor: pointer;
      }

      .transport__button svg,
      .meta-button svg,
      .icon-button svg {
        width: 15px;
        height: 15px;
      }

      .transport__button--primary {
        min-width: 54px;
        background: var(--text);
        color: #090b0e;
      }

      .player-footer {
        display: flex;
        justify-content: space-between;
        gap: 10px;
      }

      .meta-button {
        gap: 8px;
        justify-content: center;
        padding: 0 12px;
        font-size: 11px;
        font-weight: 600;
        letter-spacing: 0.08em;
        text-transform: uppercase;
      }

      [hidden] {
        display: none !important;
      }
"#;

const BODY: &str = r##"
    <section id="setup-screen" class="screen">
      <div class="setup-header">
        <div>
          <span class="eyebrow">Spotify setup</span>
          <h1 class="title">Conectar conta</h1>
          <p class="copy">Fluxo mock inteiro dentro do package. Escolha um perfil para instalar o widget com capa, faixa e controles.</p>
        </div>
        <span class="pill">Widget</span>
      </div>

      <div id="accounts" class="accounts"></div>

      <div class="setup-note">
        Esse setup eh local. O package guarda conta, faixa atual e estado de playback no proprio localStorage da instancia.
      </div>

      <button id="connect-button" class="connect-button" type="button">Conectar widget</button>
    </section>

    <section id="player-screen" class="screen player" hidden>
      <div class="player-header">
        <div>
          <span class="eyebrow">Now playing</span>
          <h1 class="title">Spotify</h1>
        </div>
        <button id="disconnect-button" class="icon-button" type="button" aria-label="Abrir setup novamente">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M8 2.5a5.5 5.5 0 1 0 5.5 5.5"></path>
            <path d="M8 1.75v3"></path>
            <path d="m10.75 2.75 1.9-1.9"></path>
          </svg>
        </button>
      </div>

      <div id="cover" class="player-cover" aria-hidden="true"></div>

      <div class="player-info">
        <p id="track-title" class="track-title"></p>
        <p id="track-artist" class="track-artist"></p>
      </div>

      <div class="progress">
        <div class="progress__bar">
          <div id="progress-fill" class="progress__fill"></div>
        </div>
        <div class="progress__meta">
          <span id="progress-current">0:00</span>
          <span id="progress-total">0:00</span>
        </div>
      </div>

      <div class="transport">
        <button id="previous-button" class="transport__button" type="button" aria-label="Faixa anterior">
          <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M12.7 3.1a.75.75 0 0 1 1.1.67v8.46a.75.75 0 0 1-1.1.66L5.2 9.22a1.4 1.4 0 0 1 0-2.44l7.5-3.68ZM2.95 3.25c.41 0 .75.34.75.75v8c0 .41-.34.75-.75.75s-.75-.34-.75-.75V4c0-.41.34-.75.75-.75Z"></path>
          </svg>
        </button>
        <button id="toggle-play-button" class="transport__button transport__button--primary" type="button" aria-label="Play ou pause">
          <svg id="play-icon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M5.3 3.2c0-.6.66-.97 1.17-.64l5.6 3.73c.47.31.47.99 0 1.3l-5.6 3.73c-.5.33-1.17-.03-1.17-.64V3.2Z"></path>
          </svg>
        </button>
        <button id="next-button" class="transport__button" type="button" aria-label="Proxima faixa">
          <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M3.3 3.1a.75.75 0 0 0-1.1.67v8.46a.75.75 0 0 0 1.1.66l7.5-3.67a1.4 1.4 0 0 0 0-2.44L3.3 3.11ZM13.05 3.25c-.41 0-.75.34-.75.75v8c0 .41.34.75.75.75s.75-.34.75-.75V4c0-.41-.34-.75-.75-.75Z"></path>
          </svg>
        </button>
      </div>

      <div class="player-footer">
        <button id="profile-pill" class="meta-button" type="button"></button>
        <button id="device-pill" class="meta-button" type="button">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <rect x="2.5" y="3.25" width="11" height="7.5" rx="1.4"></rect>
            <path d="M6 13.25h4"></path>
          </svg>
          Studio
        </button>
      </div>
    </section>
"##;

const SCRIPT: &str = r##"
      (() => {
        const instanceId =
          window.frameElement?.dataset?.packageInstanceId || "default";
        const STORAGE_KEY = `lince-spotify-package/v1/${instanceId}`;
        const accounts = [
          { id: "studio", name: "Studio account", meta: "Curadoria do board", initials: "ST" },
          { id: "personal", name: "Personal mix", meta: "Favoritos e descobertas", initials: "PM" },
          { id: "focus", name: "Focus mode", meta: "Instrumental e deep work", initials: "FM" },
        ];
        const tracks = [
          {
            title: "Night Shift",
            artist: "Lince Radio",
            duration: 198000,
            colors: ["#202735", "#e9eef5", "#8fb3a4"],
          },
          {
            title: "Quiet Frames",
            artist: "North Atelier",
            duration: 224000,
            colors: ["#1a1f28", "#d9dfe8", "#9baec6"],
          },
          {
            title: "Graphite Rain",
            artist: "South Deck",
            duration: 245000,
            colors: ["#232029", "#f0f3f7", "#b9a0c4"],
          },
          {
            title: "Signal Bloom",
            artist: "Unit Three",
            duration: 207000,
            colors: ["#1c241f", "#eef2f6", "#9fbf9f"],
          },
        ];

        const setupScreen = document.getElementById("setup-screen");
        const playerScreen = document.getElementById("player-screen");
        const accountsNode = document.getElementById("accounts");
        const connectButton = document.getElementById("connect-button");
        const disconnectButton = document.getElementById("disconnect-button");
        const cover = document.getElementById("cover");
        const trackTitle = document.getElementById("track-title");
        const trackArtist = document.getElementById("track-artist");
        const progressFill = document.getElementById("progress-fill");
        const progressCurrent = document.getElementById("progress-current");
        const progressTotal = document.getElementById("progress-total");
        const previousButton = document.getElementById("previous-button");
        const togglePlayButton = document.getElementById("toggle-play-button");
        const playIcon = document.getElementById("play-icon");
        const nextButton = document.getElementById("next-button");
        const profilePill = document.getElementById("profile-pill");

        let timer = 0;
        let state = loadState();

        function createDefaultState() {
          return {
            connected: false,
            selectedAccount: accounts[0].id,
            trackIndex: 0,
            playing: true,
            progressMs: 36000,
            startedAt: Date.now(),
          };
        }

        function loadState() {
          try {
            const raw = window.localStorage.getItem(STORAGE_KEY);
            if (!raw) {
              return createDefaultState();
            }

            const parsed = JSON.parse(raw);
            return {
              ...createDefaultState(),
              ...parsed,
              selectedAccount: accounts.some((account) => account.id === parsed?.selectedAccount)
                ? parsed.selectedAccount
                : accounts[0].id,
              trackIndex: clampTrackIndex(parsed?.trackIndex),
              playing: Boolean(parsed?.playing),
              progressMs: clampProgress(Number(parsed?.progressMs) || 0, clampTrackIndex(parsed?.trackIndex)),
              startedAt: Number(parsed?.startedAt) || Date.now(),
            };
          } catch {
            return createDefaultState();
          }
        }

        function saveState() {
          try {
            window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
          } catch {}
        }

        function clampTrackIndex(value) {
          const index = Number(value);
          if (!Number.isFinite(index) || index < 0) {
            return 0;
          }

          return index % tracks.length;
        }

        function getTrack(index) {
          return tracks[clampTrackIndex(index)];
        }

        function clampProgress(value, trackIndex) {
          const track = getTrack(trackIndex);
          return Math.max(0, Math.min(track.duration, value));
        }

        function getAccount(accountId) {
          return accounts.find((account) => account.id === accountId) || accounts[0];
        }

        function escapeHtml(value) {
          return String(value)
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#39;");
        }

        function renderArtwork(track) {
          const [base, ink, accent] = track.colors;
          return `
            <svg viewBox="0 0 320 320" fill="none" xmlns="http://www.w3.org/2000/svg">
              <rect width="320" height="320" rx="34" fill="${base}"/>
              <rect x="34" y="34" width="252" height="252" rx="26" fill="${accent}" fill-opacity="0.16"/>
              <circle cx="160" cy="132" r="76" fill="${ink}" fill-opacity="0.12"/>
              <circle cx="160" cy="132" r="48" fill="${ink}" fill-opacity="0.22"/>
              <path d="M210 88v89.4c0 16-10.8 30.6-31.2 30.6-16.1 0-28.8-9.1-28.8-23.3 0-14.3 12.1-23.1 27.9-23.1 6.2 0 12.2 1.2 16.1 3V113l-58 12.7v64.6c0 16-10.7 30.6-31.2 30.6-16 0-28.8-9.1-28.8-23.3 0-14.3 12.2-23.1 27.9-23.1 6.2 0 12.3 1.2 16.1 3v-81.6L210 88Z" fill="${ink}" fill-opacity="0.92"/>
              <path d="M77 252c32-24.7 74-36.4 126-35" stroke="${ink}" stroke-opacity="0.22" stroke-width="10" stroke-linecap="round"/>
            </svg>
          `;
        }

        function formatTime(value) {
          const totalSeconds = Math.max(0, Math.floor(value / 1000));
          const minutes = Math.floor(totalSeconds / 60);
          const seconds = totalSeconds % 60;
          return `${minutes}:${String(seconds).padStart(2, "0")}`;
        }

        function getLiveProgress() {
          const track = getTrack(state.trackIndex);
          if (!state.playing) {
            return clampProgress(state.progressMs, state.trackIndex);
          }

          const elapsed = Date.now() - state.startedAt;
          return clampProgress(state.progressMs + elapsed, state.trackIndex);
        }

        function syncTimer() {
          window.clearInterval(timer);
          timer = window.setInterval(() => {
            if (!state.connected) {
              return;
            }

            const current = getLiveProgress();
            const track = getTrack(state.trackIndex);
            if (current >= track.duration) {
              stepTrack(1);
              return;
            }

            paintProgress(current, track.duration);
          }, 250);
        }

        function paintProgress(current, duration) {
          const ratio = duration > 0 ? current / duration : 0;
          progressFill.style.transform = `scaleX(${Math.max(0, Math.min(1, ratio))})`;
          progressCurrent.textContent = formatTime(current);
          progressTotal.textContent = formatTime(duration);
        }

        function commit() {
          saveState();
          render();
        }

        function selectAccount(accountId) {
          state.selectedAccount = accountId;
          saveState();
          renderSetup();
        }

        function connect() {
          state.connected = true;
          state.playing = true;
          state.startedAt = Date.now();
          commit();
        }

        function disconnect() {
          state.connected = false;
          state.playing = false;
          state.progressMs = getLiveProgress();
          commit();
        }

        function togglePlayback() {
          if (!state.connected) {
            return;
          }

          if (state.playing) {
            state.progressMs = getLiveProgress();
            state.playing = false;
          } else {
            state.startedAt = Date.now();
            state.playing = true;
          }

          commit();
        }

        function stepTrack(direction) {
          state.trackIndex =
            (state.trackIndex + direction + tracks.length) % tracks.length;
          state.progressMs = 0;
          state.startedAt = Date.now();
          state.playing = true;
          commit();
        }

        function renderSetup() {
          const activeAccount = state.selectedAccount;
          accountsNode.innerHTML = accounts
            .map(
              (account) => `
                <button
                  class="account-option${account.id === activeAccount ? " is-selected" : ""}"
                  type="button"
                  data-account-id="${escapeHtml(account.id)}"
                >
                  <span class="account-option__avatar">${escapeHtml(account.initials)}</span>
                  <span>
                    <span class="account-option__name">${escapeHtml(account.name)}</span>
                    <span class="account-option__meta">${escapeHtml(account.meta)}</span>
                  </span>
                  <span class="account-option__check" aria-hidden="true"></span>
                </button>
              `,
            )
            .join("");
        }

        function renderPlayer() {
          const track = getTrack(state.trackIndex);
          const account = getAccount(state.selectedAccount);
          const progress = getLiveProgress();

          cover.innerHTML = renderArtwork(track);
          trackTitle.textContent = track.title;
          trackArtist.textContent = track.artist;
          playIcon.innerHTML = state.playing
            ? '<path d="M5 3.25h2.2v9.5H5Zm3.8 0H11v9.5H8.8Z"></path>'
            : '<path d="M5.3 3.2c0-.6.66-.97 1.17-.64l5.6 3.73c.47.31.47.99 0 1.3l-5.6 3.73c-.5.33-1.17-.03-1.17-.64V3.2Z"></path>';
          profilePill.textContent = account.name;
          paintProgress(progress, track.duration);
        }

        function render() {
          setupScreen.hidden = state.connected;
          playerScreen.hidden = !state.connected;

          renderSetup();

          if (state.connected) {
            renderPlayer();
          }

          syncTimer();
        }

        accountsNode.addEventListener("click", (event) => {
          const button = event.target.closest("[data-account-id]");
          if (!button) {
            return;
          }

          selectAccount(button.dataset.accountId);
        });

        connectButton.addEventListener("click", connect);
        disconnectButton.addEventListener("click", disconnect);
        previousButton.addEventListener("click", () => stepTrack(-1));
        nextButton.addEventListener("click", () => stepTrack(1));
        togglePlayButton.addEventListener("click", togglePlayback);

        render();
      })();
"##;

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "spotify-control.html",
        lang: "pt-BR",
        manifest: PackageManifest {
            icon: "♫".into(),
            title: "Spotify control".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description:
                "Widget compacto de musica com setup mock de conta, capa, faixa atual e controles.".into(),
            details:
                "Micro frontend autocontido para uma futura integracao de streaming. O host so instala e posiciona o widget; setup, player state, trocas de faixa e persistencia ficam dentro do proprio package HTML.".into(),
            initial_width: 3,
            initial_height: 4,
            requires_server: false,
            permissions: vec!["read_spotify".into(), "control_spotify".into()],
        },
        head_links: vec![],
        inline_styles: vec![STYLE],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(SCRIPT)],
    }
}

fn body() -> Markup {
    html! {
        (PreEscaped(BODY))
    }
}
