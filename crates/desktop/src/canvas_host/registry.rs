use super::*;

pub(super) fn kind_id(kind: SandKind) -> String {
    serde_json::to_value(kind)
        .unwrap()
        .as_str()
        .unwrap()
        .to_ascii_lowercase()
}

pub fn registry() -> Vec<api::Descriptor> {
    let mut result: Vec<_> = SandKind::ALL
        .into_iter()
        .map(|kind| {
            let mut settings = BTreeMap::from([(
                "texts".into(),
                api::Setting::Text {
                    default: "[]".into(),
                    max_bytes: 65536,
                },
            )]);
            settings.insert(
                "appearance".into(),
                api::Setting::Text {
                    default: "{}".into(),
                    max_bytes: 16384,
                },
            );
            if kind == SandKind::WorkTimer {
                settings.insert("time_castle".into(), api::Setting::Text {
                    default: serde_json::to_string(&lince_interface::time_castle::Settings { timezone: crate::schedule_editor::local_timezone().unwrap_or_else(|| "UTC".into()), ..default() }).unwrap(), max_bytes: 4096,
                });
                settings.insert(
                    "timer".into(),
                    api::Setting::Text {
                        default: "{\"logs\":[]}".into(),
                        max_bytes: 65536,
                    },
                );
            }
            if kind == SandKind::Todo {
                settings.insert(
                    "todo".into(),
                    api::Setting::Text {
                        default: serde_json::to_string(&crate::todo::SavedTodo::default()).unwrap(),
                        max_bytes: 65536,
                    },
                );
            }
            if matches!(
                kind,
                SandKind::Text | SandKind::EditableText | SandKind::Square
            ) {
                settings.insert(
                    "text".into(),
                    api::Setting::Text {
                        default: String::new(),
                        max_bytes: 65536,
                    },
                );
                if kind != SandKind::Square {
                    for definition in crate::sand_settings::text_definitions() {
                        let setting = match definition.control {
                            crate::sand_settings::Control::Toggle => api::Setting::Boolean {
                                default: definition.default
                                    == crate::sand_settings::Value::Toggle(true),
                            },
                            crate::sand_settings::Control::Choice(values) => api::Setting::Choice {
                                default: match definition.default {
                                    crate::sand_settings::Value::Choice(value) => value,
                                    _ => String::new(),
                                },
                                values,
                            },
                            crate::sand_settings::Control::Number { min, max, .. } => {
                                api::Setting::Number {
                                    default: match definition.default {
                                        crate::sand_settings::Value::Number(value) => {
                                            f64::from(value)
                                        }
                                        _ => 0.0,
                                    },
                                    min: f64::from(min),
                                    max: f64::from(max),
                                }
                            }
                        };
                        settings.insert(definition.id, setting);
                    }
                }
            }
            api::Descriptor {
                kind: kind_id(kind),
                name: kind.name().into(),
                description: kind.description().into(),
                settings,
                max_bindings: 0,
                placeable: true,
                configurable: true,
                composable: true,
                unavailable_reason: None,
            }
        })
        .collect();
    for (kind, name) in [
        ("protein", "Protein Castle"),
        ("kanban", "Kanban"),
        ("instinct", "Instinct"),
        ("simulation", "Simulation"),
        ("drawing", "Drawing"),
        ("recorder", "Recorder"),
        ("document", "Document viewer"),
        ("media", "Media"),
        ("ide", "Editor"),
        ("explorer", "File explorer"),
        ("shader", "Shader"),
        ("assertion", "Assertion"),
        ("layout", "Layout"),
        ("import", "Imported object"),
        ("unavailable", "Native placement"),
    ] {
        result.push(api::Descriptor { kind: kind.into(), name: name.into(), description: format!("Existing native {name}; geometry and workspace can be edited."), settings: BTreeMap::new(), max_bindings: 0, placeable: false, configurable: false, composable: false, unavailable_reason: Some("Its native state does not yet have a shared composition schema; use the existing picker for creation.".into()) });
    }
    result.retain(|descriptor| !matches!(descriptor.kind.as_str(), "protein" | "instinct"));
    result.push(api::Descriptor { kind: "protein".into(), name: "Protein Castle".into(), description: "A live query and its result components. query is the ordinary Protein JSON, with referenced Record UIDs also listed in bindings.".into(), settings: BTreeMap::from([("name".into(), api::Setting::Text { default: "Protein".into(), max_bytes: 1024 }), ("slug".into(), api::Setting::Text { default: String::new(), max_bytes: 1024 }), ("query".into(), api::Setting::Text { default: serde_json::to_string(&crate::protein_castle::ProteinDraft::default().query).unwrap(), max_bytes: 65536 })]), max_bindings: 32, placeable: true, configurable: true, composable: true, unavailable_reason: None });
    result.push(api::Descriptor {
        kind: "instinct".into(),
        name: "Instinct".into(),
        description: "Read an existing Instinct page. An empty page uses the existing library."
            .into(),
        settings: BTreeMap::from([(
            "page".into(),
            api::Setting::Text {
                default: String::new(),
                max_bytes: 256,
            },
        )]),
        max_bindings: 0,
        placeable: true,
        configurable: true,
        composable: true,
        unavailable_reason: None,
    });
    result
}
