use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, PreEscaped, html};

pub(crate) const FEATURE_FLAG: &str = "sand.weather";

const STYLE: &str = r#"
      :root {
        color-scheme: dark;
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        --bg: #151a21;
        --bg-soft: #1b2129;
        --text: #f0f4f8;
        --text-soft: #c8d0da;
        --text-muted: #8e97a3;
        --sun: #f3c86d;
        --rain: #8cc8ff;
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
        grid-template-rows: auto 1fr auto;
        gap: 14px;
        min-height: 100vh;
        padding: 16px;
        background: var(--bg);
        color: var(--text);
      }

      .location {
        display: grid;
        gap: 4px;
      }

      .city {
        margin: 0;
        font-size: 15px;
        font-weight: 600;
        letter-spacing: -0.02em;
      }

      .region {
        margin: 0;
        color: var(--text-muted);
        font-size: 12px;
      }

      .hero {
        display: grid;
        align-content: center;
        justify-items: center;
        gap: 14px;
        text-align: center;
      }

      .visual {
        display: grid;
        place-items: center;
        width: min(100%, 146px);
        aspect-ratio: 1 / 1.08;
        border-radius: 28px;
        background: var(--bg-soft);
      }

      .visual svg {
        width: 106px;
        height: 118px;
      }

      .sun {
        transform-origin: 40px 28px;
        animation: pulse 4.8s ease-in-out infinite;
      }

      .cloud {
        animation: drift 6s ease-in-out infinite;
      }

      .drop {
        animation: rain 2.4s ease-in-out infinite;
      }

      .temperature {
        margin: 0;
        font-size: 64px;
        line-height: 0.9;
        letter-spacing: -0.08em;
      }

      .condition {
        margin: 0;
        color: var(--text-soft);
        font-size: 13px;
      }

      .footer {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 10px;
      }

      .stat {
        display: grid;
        gap: 4px;
        padding: 10px 0;
      }

      .stat__label {
        color: var(--text-muted);
        font-size: 10px;
        font-weight: 600;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }

      .stat__value {
        color: var(--text);
        font-size: 13px;
        font-weight: 600;
      }

      @keyframes pulse {
        0%,
        100% {
          transform: scale(1);
        }

        50% {
          transform: scale(1.05);
        }
      }

      @keyframes drift {
        0%,
        100% {
          transform: translateX(0);
        }

        50% {
          transform: translateX(4px);
        }
      }

      @keyframes rain {
        0%,
        100% {
          transform: translateY(0);
          opacity: 0.88;
        }

        50% {
          transform: translateY(4px);
          opacity: 1;
        }
      }
"#;

const BODY: &str = r##"
    <header class="location">
      <h1 class="city">Sao Paulo</h1>
      <p class="region">Brasil</p>
    </header>

    <main class="hero">
      <div class="visual" aria-hidden="true">
        <svg viewBox="0 0 100 120" fill="none">
          <circle class="sun" cx="40" cy="28" r="14" fill="#F3C86D" />
          <g class="cloud">
            <path
              d="M28 62c0-8.28 6.72-15 15-15 6.45 0 11.95 4.08 14.11 9.82A13.18 13.18 0 0 1 60 57c7.18 0 13 5.82 13 13s-5.82 13-13 13H31c-7.73 0-14-6.27-14-14s6.27-14 14-14c.23 0 .45.01.67.02A14.94 14.94 0 0 1 28 62Z"
              fill="#ECF1F7"
            />
          </g>
          <path class="drop" d="M38 90c3-4 4-6 4-8a4 4 0 1 0-8 0c0 2 1 4 4 8Z" fill="#8CC8FF" />
          <path class="drop" d="M52 95c3-4 4-6 4-8a4 4 0 1 0-8 0c0 2 1 4 4 8Z" fill="#8CC8FF" style="animation-delay:.45s" />
          <path class="drop" d="M66 90c3-4 4-6 4-8a4 4 0 1 0-8 0c0 2 1 4 4 8Z" fill="#8CC8FF" style="animation-delay:.85s" />
        </svg>
      </div>

      <div>
        <p class="temperature">22&deg;</p>
        <p class="condition">chuva leve agora</p>
      </div>
    </main>

    <footer class="footer">
      <div class="stat">
        <span class="stat__label">Max / Min</span>
        <span class="stat__value">24&deg; / 18&deg;</span>
      </div>
      <div class="stat">
        <span class="stat__label">Chance</span>
        <span class="stat__value">64% chuva</span>
      </div>
    </footer>
"##;

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "previsao-do-tempo.html",
        lang: "pt-BR",
        manifest: PackageManifest {
            icon: "☁".into(),
            title: "Previsao do tempo".into(),
            author: "Lince Labs".into(),
            version: "0.2.0".into(),
            description:
                "Card vertical de clima, mais compacto, com ilustracao e leitura resumida.".into(),
            details:
                "Widget de clima pensado como micro frontend visual. O HTML assume toda a superficie do card e entrega uma leitura direta: cidade, temperatura atual e um estado atmosferico sintetico.".into(),
            initial_width: 3,
            initial_height: 4,
            requires_server: false,
            permissions: vec!["read_weather".into(), "read_location".into()],
        },
        head_links: vec![],
        inline_styles: vec![STYLE],
        body: body(),
        body_scripts: vec![],
    }
}

fn body() -> Markup {
    html! {
        (PreEscaped(BODY))
    }
}
