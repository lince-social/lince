use {
    crate::sand::{render_trail_body, render_trail_script, TRAIL_INLINE_STYLES},
    maud::{DOCTYPE, Markup, PreEscaped, html},
    serde_json::Value,
};

use super::shared::{asset_version_token, safe_json_for_html};

pub fn render_trail_page(instance_id: &str, trail_contract: &Value) -> String {
    let asset_version = asset_version_token();
    let stream_url = format!("/host/trail/{instance_id}/stream");
    let trail_signals = safe_json_for_html(&wrap_trail_signals(trail_contract, &stream_url));
    let trail_script = render_trail_script();
    let has_trail_root = trail_has_root(trail_contract);

    render_trail_document(
        instance_id,
        asset_version,
        &trail_signals,
        &stream_url,
        has_trail_root,
        &trail_script,
    )
    .into_string()
}

fn render_trail_document(
    instance_id: &str,
    asset_version: u64,
    trail_signals: &str,
    stream_url: &str,
    has_trail_root: bool,
    trail_script: &str,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="pt-BR" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Trail Relation" }
                link rel="stylesheet" href=(format!("/static/styles.css?v={asset_version}"));
                @for style_block in TRAIL_INLINE_STYLES {
                    style { (PreEscaped(*style_block)) }
                }
                script type="module" src=(format!("/static/vendored/datastar.js?v={asset_version}")) {}
                script src=(format!("/static/vendored/d3.v7.min.js?v={asset_version}")) {}
            }
            body
                class="trail-page"
                data-signals=(trail_signals)
                data-on-signal-patch="window.TrailWidget?.syncFromSignals?.(patch)"
                data-on-signal-patch-filter="{include: /^trail(\\.|$)/}"
            {
                @if has_trail_root {
                    div
                        id="trail-stream-bootstrap"
                        class="trail-page__bootstrap"
                        data-init=(format!("@get('{stream_url}')"))
                        hidden=""
                    {}
                } @else {
                    div
                        id="trail-stream-bootstrap"
                        class="trail-page__bootstrap"
                        hidden=""
                    {}
                }
                (render_trail_body())
                script { (PreEscaped(trail_script)) }
                div class="trail-page__instance" hidden="" { (instance_id) }
            }
        }
    }
}

fn wrap_trail_signals(trail_contract: &Value, stream_url: &str) -> Value {
    let mut trail = trail_contract.clone();
    if let Some(object) = trail.as_object_mut() {
        object.insert(
            "stream".into(),
            serde_json::json!({
                "status": "idle",
                "error": null,
                "url": stream_url,
            }),
        );
    }

    serde_json::json!({
        "trail": trail,
    })
}

fn trail_has_root(trail_contract: &Value) -> bool {
    trail_contract
        .get("binding")
        .and_then(|binding| binding.get("trailRootRecordId"))
        .and_then(Value::as_i64)
        .is_some()
}
