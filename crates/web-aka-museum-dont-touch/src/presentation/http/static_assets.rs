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
        bytes: include_bytes!("../../../../../assets/logo/black_in_white.ico"),
        content_type: "image/x-icon",
    })
}

pub async fn frame_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../static/presentation/board/frame.js"
    )))
}

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

pub async fn vault_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../static/presentation/board/vault.js"
    )))
}

pub async fn collab_editor_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../static/presentation/board/collab-editor.js"
    )))
}

pub async fn lynx_ui_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../static/presentation/board/lynx-ui.js"
    )))
}

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

pub async fn loro_index_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../src/sand/collab/vendor/loro-index.js"
    )))
}

pub async fn loro_wasm_js() -> Response {
    asset_response(js(include_bytes!(
        "../../../src/sand/collab/vendor/loro_wasm.js"
    )))
}

pub async fn loro_wasm_bg() -> Response {
    asset_response(EmbeddedAsset {
        bytes: include_bytes!("../../../src/sand/collab/vendor/loro_wasm_bg.wasm"),
        content_type: "application/wasm",
    })
}

pub async fn loro_license() -> Response {
    asset_response(text(include_bytes!(
        "../../../src/sand/collab/vendor/LoroLicense"
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
        "presentation/board/transport.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/transport.js"
        ))),
        "presentation/board/frame.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/frame.js"
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
