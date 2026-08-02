use axum::{
    body::Body,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::Response,
};

struct EmbeddedAsset {
    bytes: &'static [u8],
    content_type: &'static str,
}

pub async fn serve(Path(path): Path<String>) -> Response {
    embedded_asset(&path).map_or_else(not_found, asset_response)
}

pub async fn favicon() -> Response {
    asset_response(EmbeddedAsset {
        bytes: include_bytes!("../../../../../assets/black_in_white.ico"),
        content_type: "image/x-icon",
    })
}

/// The new-way sand host (`frame.js`), served at the absolute `/board/frame.js`
/// that every migrated sand loads. Runs inside the sand iframe (srcdoc, so the
/// URL resolves against the board origin) and exposes `window.LinceWidgetHost`
/// (Protein subscriptions + Actions + onLive) talking to the board-side unified
/// widget bridge over the one shared transport WebSocket.
pub async fn frame_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../static/presentation/board/frame.js"
    )))
}

/// The reusable body editor (K6): slash block palette + @-mention picker +
/// the shared markdown block renderer, served at the absolute
/// `/board/editor.js` beside frame.js. Sands that show or edit a record body
/// load it and get `window.LinceBodyEditor`.
pub async fn editor_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../static/presentation/board/editor.js"
    )))
}

pub async fn lynx_ui_css() -> Response {
    asset_response(css(include_bytes!(
        "../../../static/presentation/board/lynx-ui.css"
    )))
}

pub async fn lynx_ui_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../static/presentation/board/lynx-ui.js"
    )))
}

/// Vendored d3 v7 for the relations force graph, served at the absolute
/// `/board/vendor/d3.v7.min.js`. This MUST be an always-registered route like
/// frame.js/editor.js: when `static_dir` exists on disk, `/static/*` goes to
/// ServeDir alone and the embedded fallback below is never wired — d3 under
/// `/static/vendored/` 404'd there and the graph lost all physics. The
/// license travels beside it (AGENTS.md rule).
pub async fn d3_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../src/sand/relations/d3.v7.min.js"
    )))
}

pub async fn d3_license() -> Response {
    asset_response(text(include_bytes!(
        "../../../src/sand/relations/LICENSE.txt"
    )))
}

/// Vendored mermaid v11 for the Instinct sand's diagrams, served at the
/// absolute `/board/vendor/mermaid.min.js`. Same reason as d3 above: this MUST
/// stay an always-registered route, because when `static_dir` exists on disk
/// `/static/*` is handled by ServeDir alone and anything under
/// `/static/vendored/` 404s. The license travels beside it (AGENTS.md rule).
///
/// This is the UMD `dist/mermaid.min.js` on purpose — a single self-contained
/// bundle with NO dynamic imports, which assigns `globalThis.mermaid`. The
/// `.esm.mjs` builds code-split into ~1000 chunk files and cannot be served
/// from one embedded route.
pub async fn mermaid_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../src/sand/instinct/mermaid.min.js"
    )))
}

pub async fn mermaid_license() -> Response {
    asset_response(text(include_bytes!(
        "../../../src/sand/instinct/LICENSE.txt"
    )))
}

fn embedded_asset(path: &str) -> Option<EmbeddedAsset> {
    match path {
        "styles.css" => Some(css(include_bytes!("../../../static/styles.css"))),
        "ai-builder.css" => Some(css(include_bytes!("../../../static/ai-builder.css"))),
        "lince_logo_white.svg" => Some(svg(include_bytes!("../../../static/lince_logo_white.svg"))),
        "presentation/ai/main.js" => Some(js(include_bytes!(
            "../../../static/presentation/ai/main.js"
        ))),
        "presentation/board/grid.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/grid.js"
        ))),
        "presentation/board/interactions.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/interactions.js"
        ))),
        "presentation/board/LynxDS-components.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/LynxDS-components.js"
        ))),
        "presentation/board/lynx-ui.css" => Some(css(include_bytes!(
            "../../../static/presentation/board/lynx-ui.css"
        ))),
        "presentation/board/lynx-ui.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/lynx-ui.js"
        ))),
        "presentation/board/main.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/main.js"
        ))),
        "presentation/board/store.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/store.js"
        ))),
        "presentation/board/widget-bridge.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/widget-bridge.js"
        ))),
        // The board's single shared transport socket (Stage 8b, base task 1):
        // the one WebSocket that the unified widget bridge and the Data-panel
        // Protein config multiplex over.
        "presentation/board/transport.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/transport.js"
        ))),
        // New-way sand host, also served at `/board/frame.js` (see `frame_js`).
        "presentation/board/frame.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/frame.js"
        ))),
        "vendored/datastar.js" => Some(js(include_bytes!("../../../static/vendored/datastar.js"))),
        "vendored/DatastarReference" => Some(text(include_bytes!(
            "../../../static/vendored/DatastarReference"
        ))),
        _ => None,
    }
}

fn asset_response(asset: EmbeddedAsset) -> Response {
    let mut response = Response::new(Body::from(asset.bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(asset.content_type),
    );
    // These embedded board assets (main.js, store.js, widget-bridge.js, …) are
    // rebuilt in place during development. Without a revalidation header the
    // Tauri/desktop webview happily serves a STALE copy, which shows up as a
    // NEW main.js calling a method a stale store.js hasn't got yet
    // ("store.addImportedGroup is not a function"). Force revalidation so the
    // whole board's JS is always coherent.
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, must-revalidate"),
    );
    response
}

fn not_found() -> Response {
    let mut response = Response::new(Body::from("Not found"));
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

const fn css(bytes: &'static [u8]) -> EmbeddedAsset {
    EmbeddedAsset {
        bytes,
        content_type: "text/css; charset=utf-8",
    }
}

const fn js(bytes: &'static [u8]) -> EmbeddedAsset {
    EmbeddedAsset {
        bytes,
        content_type: "text/javascript; charset=utf-8",
    }
}

const fn svg(bytes: &'static [u8]) -> EmbeddedAsset {
    EmbeddedAsset {
        bytes,
        content_type: "image/svg+xml",
    }
}

const fn text(bytes: &'static [u8]) -> EmbeddedAsset {
    EmbeddedAsset {
        bytes,
        content_type: "text/plain; charset=utf-8",
    }
}
