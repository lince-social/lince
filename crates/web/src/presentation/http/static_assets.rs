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
        "presentation/board/main.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/main.js"
        ))),
        "presentation/board/store.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/store.js"
        ))),
        "presentation/board/widget-bridge.js" => Some(js(include_bytes!(
            "../../../static/presentation/board/widget-bridge.js"
        ))),
        "vendored/d3.v7.min.js" => Some(js(include_bytes!(
            "../../../src/sand/relations/d3.v7.min.js"
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
