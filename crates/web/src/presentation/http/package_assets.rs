//! Serving a sand package's assets (entry HTML, manifest, bundled files) with
//! the host bootstrap injected. Extracted from the retired legacy packages API;
//! this is the only piece the Cell surface needs.

use {
    crate::domain::lince_package::{LincePackage, normalize_asset_path},
    crate::presentation::http::api_error::{ApiResult, api_error},
    axum::{
        body::Body,
        http::{HeaderMap, HeaderValue, StatusCode, header},
        response::{IntoResponse, Response},
    },
};

const DATASTAR_BOOTSTRAP_SCRIPT: &str =
    r#"<script type="module" src="/host/static/vendored/datastar.js"></script>"#;
const WIDGET_BOOTSTRAP_SCRIPT: &str =
    r#"<script src="/host/static/presentation/board/widget-frame-bootstrap.js"></script>"#;

pub(crate) fn serve_package_asset(
    package: &LincePackage,
    asset_path: &str,
    content_root_url: &str,
) -> ApiResult<Response> {
    let asset_path = normalize_asset_path(asset_path)
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message))?;

    let (bytes, content_type) = if asset_path == "index.html" || asset_path == package.entry_path()
    {
        (
            inject_package_html(
                &package.html_document(),
                package.entry_path(),
                content_root_url,
            )
            .into_bytes(),
            "text/html; charset=utf-8",
        )
    } else if asset_path == "config.toml" {
        (
            package
                .manifest_toml()
                .map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?
                .into_bytes(),
            "application/toml; charset=utf-8",
        )
    } else if let Some(bytes) = package.asset_bytes(&asset_path) {
        (bytes.to_vec(), content_type_for_path(&asset_path))
    } else {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "Esse arquivo interno do widget nao existe.",
        ));
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    Ok((headers, Body::from(bytes)).into_response())
}

fn inject_package_html(raw_html: &str, entry_path: &str, content_root_url: &str) -> String {
    if raw_html.contains("window.__LINCE_WIDGET_HOST__")
        || raw_html.contains("widget-frame-bootstrap.js")
    {
        return ensure_base_href(raw_html, entry_path, content_root_url);
    }

    let datastar_script = if raw_html.contains("datastar.js") {
        ""
    } else {
        DATASTAR_BOOTSTRAP_SCRIPT
    };
    let injections = [datastar_script, WIDGET_BOOTSTRAP_SCRIPT]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let html = ensure_base_href(raw_html, entry_path, content_root_url);

    if html.contains("</head>") {
        return html.replace("</head>", &format!("{injections}\n</head>"));
    }

    if html.contains("<body") {
        return html.replacen("<body", &format!("{injections}\n<body"), 1);
    }

    if html.contains("<script") {
        return html.replacen("<script", &format!("{injections}\n<script"), 1);
    }

    if html.contains("</html>") {
        return html.replace("</html>", &format!("{injections}\n</html>"));
    }

    format!("{injections}\n{html}")
}

fn content_type_for_path(asset_path: &str) -> &'static str {
    match path_extension_lower(asset_path).as_deref() {
        Some("css") => "text/css; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("gif") => "image/gif",
        Some("htm") | Some("html") => "text/html; charset=utf-8",
        Some("jpeg") | Some("jpg") => "image/jpeg",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("md") | Some("txt") => "text/plain; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("toml") => "application/toml; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("webp") => "image/webp",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn path_extension_lower(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn ensure_base_href(raw_html: &str, entry_path: &str, content_root_url: &str) -> String {
    let parent_path = std::path::Path::new(entry_path)
        .parent()
        .and_then(|value| value.to_str())
        .map(|value| value.trim_matches('/'))
        .filter(|value| !value.is_empty());

    let Some(parent_path) = parent_path else {
        return raw_html.to_string();
    };

    if raw_html.to_ascii_lowercase().contains("<base ") {
        return raw_html.to_string();
    }

    let href = format!("{content_root_url}/{}/", encode_asset_path(parent_path));
    let base_tag = format!("<base href=\"{href}\">");

    if let Some(head_open_start) = raw_html.to_ascii_lowercase().find("<head")
        && let Some(relative_head_end) = raw_html[head_open_start..].find('>')
    {
        let insert_at = head_open_start + relative_head_end + 1;
        let mut updated = String::with_capacity(raw_html.len() + base_tag.len() + 1);
        updated.push_str(&raw_html[..insert_at]);
        updated.push('\n');
        updated.push_str(&base_tag);
        updated.push_str(&raw_html[insert_at..]);
        return updated;
    }

    format!("{base_tag}\n{raw_html}")
}

fn encode_asset_path(asset_path: &str) -> String {
    asset_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(urlencoding::encode)
        .map(|segment| segment.into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
