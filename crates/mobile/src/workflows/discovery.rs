use super::*;

fn social(
    world: &mut World,
    parent: Entity,
    caption: &str,
    request: Value,
    fields: &[(&str, &str, FieldKind)],
) {
    let mut form = Form::action(caption, json!({"action":"social","request":request}));
    form.data.as_object_mut().unwrap().remove("request_id");
    form.actor = Some(person(world));
    form.mode = "social".into();
    form.review = json!({"Request":request,"Privacy":"Only the displayed public fields and configured endpoints are used. This does not reveal the private Transfer or start location sharing. Network coverage can be incomplete."});
    for (path, label, kind) in fields {
        form.field(path, label, kind.clone());
    }
    button(world, parent, caption, Intent::Open(form));
}

fn search(world: &mut World, parent: Entity, transport: Value) {
    social(
        world,
        parent,
        "Find public Transport announcements",
        json!({"command":"search","query":{"text":"","direction":"need","area":"","transport":transport},"services":[]}),
        &[
            ("/request/query/text", "Words to find", FieldKind::Text),
            (
                "/request/query/direction",
                "Need or contribution",
                FieldKind::Choice(vec!["need".into(), "contribution".into()]),
            ),
            (
                "/request/query/area",
                "Declared city or region",
                FieldKind::Text,
            ),
            (
                "/request/services",
                "Optional directory endpoint IDs (comma-separated)",
                FieldKind::People,
            ),
        ],
    );
}

pub(super) fn entry(world: &mut World, parent: Entity, transfer: Option<&Value>) {
    search(world, parent, json!({"activity":"ride"}));
    if let Some(transfer) = transfer
        && array(transfer, "promises")
            .iter()
            .any(|promise| promise["transport"].is_object())
    {
        social(
            world,
            parent,
            "Prepare public Transport announcement",
            json!({"command":"prepare-from-record","source":transfer["uid"]}),
            &[],
        );
    }
}

pub(super) fn results(world: &mut World, parent: Entity, result: &Value) {
    if result["draft"]["transport"].is_object() {
        let mut draft = result["draft"].clone();
        if draft["destinations"].is_null() {
            draft["destinations"] = json!([]);
        }
        social(
            world,
            parent,
            "Save public announcement draft",
            json!({"command":"save-draft","record":result["record"],"source":result["source"],"draft":draft}),
            &[
                ("/request/draft/title", "Public title", FieldKind::Text),
                ("/request/draft/text", "Public description", FieldKind::Text),
                (
                    "/request/draft/mode",
                    "Public identity",
                    FieldKind::Choice(vec!["anonymous".into(), "identified".into()]),
                ),
                (
                    "/request/draft/alias",
                    "Anonymous alias (optional)",
                    FieldKind::Text,
                ),
                (
                    "/request/draft/transport/origin/radius_metres",
                    "Public origin radius (at least 1000 metres)",
                    FieldKind::Number,
                ),
                (
                    "/request/draft/transport/destination/radius_metres",
                    "Public destination radius (at least 1000 metres)",
                    FieldKind::Number,
                ),
                (
                    "/request/draft/transport/immediate",
                    "Immediate (expires in 15 minutes)",
                    FieldKind::Choice(vec!["true".into(), "false".into()]),
                ),
                (
                    "/request/draft/destinations",
                    "Publication endpoint IDs (comma-separated)",
                    FieldKind::People,
                ),
            ],
        );
        let route = &draft["transport"];
        search(
            world,
            parent,
            json!({"activity":route["activity"],"origin":route["origin"],"destination":route["destination"],"window_start":route["window_start"],"window_end":route["window_end"]}),
        );
    }
    if result["status"] == "draft" {
        social(
            world,
            parent,
            "Preview public announcement",
            json!({"command":"preview","record":result["record"],"state":"active"}),
            &[],
        );
    }
    if result["preview_hash"].is_string() && result["document"].is_object() {
        social(
            world,
            parent,
            "Publish this exact public preview",
            json!({"command":"publish","record":result["record"],"preview_hash":result["preview_hash"],"document":result["document"]}),
            &[],
        );
    }
    for row in array(result, "results") {
        let document = &row["document"];
        app::label(
            world,
            parent,
            document["title"].as_str().unwrap_or("Public Transport"),
            18.0,
        );
        if let Ok(route) = serde_json::from_value::<nucleus::social::transport::Announcement>(
            document["transport"].clone(),
        ) {
            app::button(
                world,
                parent,
                "View public approximate route",
                app::Intent::Map(crate::map::Intent::PublicRoute(
                    route,
                    document["expires_at"].as_i64().unwrap_or_default(),
                )),
            );
        }
        if document["state"] == "active" && document["reply"].is_object() {
            social(
                world,
                parent,
                "Write a private introduction",
                json!({"command":"open-request","post":document,"text":"","alias":"","services":[]}),
                &[
                    ("/request/text", "Private introduction", FieldKind::Text),
                    (
                        "/request/alias",
                        "Private alias (optional)",
                        FieldKind::Text,
                    ),
                    (
                        "/request/services",
                        "Optional mailbox endpoint IDs (comma-separated)",
                        FieldKind::People,
                    ),
                ],
            );
        }
    }
}
