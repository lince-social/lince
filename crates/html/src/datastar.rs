use axum::response::sse::Event;
use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

fn write_event_data(buffer: &mut String, key: &str, value: &str) {
    for line in value.lines() {
        buffer.push_str(key);
        buffer.push(' ');
        buffer.push_str(line);
        buffer.push('\n');
    }
    if value.is_empty() {
        buffer.push_str(key);
        buffer.push_str(" \n");
    }
}

fn write_sse_data(buffer: &mut String, key: &str, value: &str) {
    for line in value.lines() {
        buffer.push_str("data: ");
        buffer.push_str(key);
        buffer.push(' ');
        buffer.push_str(line);
        buffer.push('\n');
    }
    if value.is_empty() {
        buffer.push_str("data: ");
        buffer.push_str(key);
        buffer.push_str(" \n");
    }
}

pub fn patch_elements(selector: &str, mode: &str, elements: String) -> Response {
    let body = patch_elements_event_body(selector, mode, elements);

    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

pub fn patch_elements_event_body(selector: &str, mode: &str, elements: String) -> String {
    let mut body = String::new();
    body.push_str("event: datastar-patch-elements\n");
    write_sse_data(&mut body, "selector", selector);
    write_sse_data(&mut body, "mode", mode);
    write_sse_data(&mut body, "elements", &elements);
    body.push('\n');
    body
}

pub fn patch_elements_event(selector: &str, mode: &str, elements: String) -> Event {
    let mut payload = String::new();
    write_event_data(&mut payload, "selector", selector);
    write_event_data(&mut payload, "mode", mode);
    write_event_data(&mut payload, "elements", &elements);
    Event::default()
        .event("datastar-patch-elements")
        .data(payload)
}
