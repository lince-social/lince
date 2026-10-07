use super::*;
use serde_json::json;

fn rule(target: Target, conditions: Vec<Condition>, colors: Colors) -> Rule {
    Rule {
        name: "Test".into(),
        enabled: true,
        target,
        mode: Mode::All,
        conditions,
        colors,
    }
}

fn quantity(side: RecordSide, operator: Operator, value: &str) -> Condition {
    Condition::Quantity {
        record: side,
        comparison: Comparison {
            operator,
            value: value.into(),
            ..default()
        },
        unit: Unit::Any,
    }
}

fn card(settings: &Settings, record: &Value) -> Colors {
    settings.evaluate(
        Target::Card,
        record,
        &Value::Null,
        &Value::Null,
        &Value::Null,
    )
}

#[test]
fn exact_comparisons_ranges_and_missing_values() {
    for (operator, value, expected) in [
        (Operator::Equal, "9007199254740993.1250", true),
        (Operator::NotEqual, "9007199254740993.126", true),
        (Operator::Greater, "9007199254740993.124", true),
        (Operator::AtLeast, "9007199254740993.125", true),
        (Operator::Less, "9007199254740993.125", false),
        (Operator::AtMost, "9007199254740993.125", true),
    ] {
        let comparison = Comparison {
            operator,
            value: value.into(),
            ..default()
        };
        assert!(comparison.valid());
        assert_eq!(comparison.matches(&json!("9007199254740993.125")), expected);
        assert!(!comparison.matches(&Value::Null));
        assert!(!comparison.matches(&json!("invalid")));
    }
    let mut range = Comparison {
        operator: Operator::Between,
        value: "-2".into(),
        upper: "2".into(),
        ..default()
    };
    assert!(range.matches(&json!("-2")) && range.matches(&json!("2")));
    range.include_lower = false;
    range.include_upper = false;
    assert!(!range.matches(&json!("-2")) && !range.matches(&json!("2")));
    assert!(range.matches(&json!("0")));
    range.upper = "-2".into();
    assert!(!range.valid());
    range.include_lower = true;
    range.include_upper = true;
    assert!(range.valid());
    range.upper = "-3".into();
    assert!(!range.valid());
}

#[test]
fn first_match_per_property_all_any_and_disabled_rules() {
    let red = Color::Specific([255, 0, 0, 255]);
    let blue = Color::Specific([0, 0, 255, 255]);
    let mut settings = Settings {
        rules: vec![
            rule(
                Target::Card,
                vec![quantity(RecordSide::Record, Operator::Greater, "5")],
                Colors {
                    background: Some(red.clone()),
                    ..default()
                },
            ),
            rule(
                Target::Card,
                vec![],
                Colors {
                    background: Some(blue.clone()),
                    text: Some(blue.clone()),
                    ..default()
                },
            ),
        ],
    };
    assert!(settings.valid());
    let matched = card(&settings, &json!({"quantity":"10"}));
    assert_eq!(matched.background, Some(red));
    assert_eq!(matched.text, Some(blue.clone()));
    settings.rules[0]
        .conditions
        .push(quantity(RecordSide::Record, Operator::Less, "0"));
    assert_eq!(
        card(&settings, &json!({"quantity":"10"})).background,
        Some(blue.clone())
    );
    settings.rules[0].mode = Mode::Any;
    assert_ne!(
        card(&settings, &json!({"quantity":"10"})).background,
        Some(blue.clone())
    );
    settings.rules[0].enabled = false;
    assert_eq!(
        card(&settings, &json!({"quantity":"10"})).background,
        Some(blue)
    );
}

#[test]
fn assertions_match_individual_values_direction_family_and_units() {
    let record = json!({"uid":"one", "relation_context":{"assertions":[
        {"from":"one","to":"outside","predicate_uid":"c_child","predicate":"child","families":["parent"],"quantity":"3","unit":"c_kg","unit_name":"kg"},
        {"from":"one","to":null,"predicate":"child","families":["parent"],"quantity":null,"unit":null},
        {"from":"outside","to":"one","predicate":"child","families":["parent"],"quantity":"7","unit":"c_kg","unit_name":"kg"}
    ]}});
    let mut settings = Settings {
        rules: vec![rule(
            Target::Card,
            vec![Condition::Assertion {
                record: RecordSide::Record,
                predicate: "parent".into(),
                family: true,
                direction: Direction::Outgoing,
                present: true,
                quantity: Some(Comparison {
                    operator: Operator::Greater,
                    value: "5".into(),
                    ..default()
                }),
                unit: Unit::Exact("kg".into()),
            }],
            Colors {
                border: Some(Color::Token(Token::Warning)),
                ..default()
            },
        )],
    };
    assert_eq!(card(&settings, &record), Colors::default());
    if let Condition::Assertion { direction, .. } = &mut settings.rules[0].conditions[0] {
        *direction = Direction::Incoming;
    }
    assert!(card(&settings, &record).border.is_some());
    if let Condition::Assertion {
        predicate,
        family,
        quantity,
        direction,
        unit,
        ..
    } = &mut settings.rules[0].conditions[0]
    {
        *predicate = "child".into();
        *family = false;
        *quantity = None;
        *direction = Direction::Outgoing;
        *unit = Unit::Unitless;
    }
    assert!(card(&settings, &record).border.is_some());
    if let Condition::Assertion { direction, .. } = &mut settings.rules[0].conditions[0] {
        *direction = Direction::Incoming;
    }
    assert_eq!(card(&settings, &record), Colors::default());
    if let Condition::Assertion { present, .. } = &mut settings.rules[0].conditions[0] {
        *present = false;
    }
    assert!(card(&settings, &record).border.is_some());
    assert_eq!(card(&settings, &json!({"uid":"one"})), Colors::default());
}

#[test]
fn link_rules_match_assertion_and_both_endpoints() {
    let settings = Settings {
        rules: vec![rule(
            Target::Link,
            vec![
                quantity(RecordSide::Source, Operator::AtLeast, "10"),
                quantity(RecordSide::Destination, Operator::Equal, "2"),
                Condition::Link {
                    predicate: "depends-on".into(),
                    family: false,
                    quantity: Some(Comparison {
                        operator: Operator::Greater,
                        value: "1".into(),
                        ..default()
                    }),
                    unit: Unit::Any,
                },
            ],
            Colors {
                link: Some(Color::Token(Token::Warning)),
                ..default()
            },
        )],
    };
    assert!(settings.valid());
    let source = json!({"quantity":"10"});
    let destination = json!({"quantity":"2"});
    let link = json!({"predicate":"depends-on","quantity":"1.01"});
    assert!(
        settings
            .evaluate(Target::Link, &Value::Null, &source, &destination, &link)
            .link
            .is_some()
    );
    assert_eq!(
        settings.evaluate(
            Target::Link,
            &Value::Null,
            &source,
            &destination,
            &json!({"predicate":"depends-on","quantity":null})
        ),
        Colors::default()
    );
    assert_eq!(
        settings.evaluate(
            Target::Link,
            &Value::Null,
            &source,
            &json!({"quantity":"3"}),
            &link
        ),
        Colors::default()
    );
}

#[test]
fn validation_rejects_wrong_targets_noncolor_tokens_and_excessive_rules() {
    let mut settings = Settings {
        rules: vec![rule(
            Target::Card,
            vec![],
            Colors {
                background: Some(Color::Token(Token::BorderWidth)),
                ..default()
            },
        )],
    };
    assert!(!settings.valid());
    settings.rules[0].colors = Colors {
        background: Some(Color::Token(Token::Warning)),
        ..default()
    };
    settings.rules[0]
        .conditions
        .push(quantity(RecordSide::Source, Operator::Equal, "1"));
    assert!(!settings.valid());
    settings.rules[0].conditions = vec![Condition::Assertion {
        record: RecordSide::Record,
        predicate: " parent ".into(),
        family: true,
        direction: Direction::Either,
        present: true,
        quantity: None,
        unit: Unit::Any,
    }];
    assert!(settings.valid());
    assert_eq!(settings.families(), ["parent"]);
    settings.rules = vec![settings.rules[0].clone(); 129];
    assert!(!settings.valid());
}

#[test]
fn overlays_inherit_follow_tokens_preserve_borders_and_restore_manual_colors() {
    let mut world = World::new();
    world.init_resource::<crate::tokens::ThemeSettings>();
    let manual = crate::tokens::TokenOverrides(std::collections::BTreeMap::from([
        (Token::SandBackground, TokenValue::Color([1, 2, 3, 255])),
        (Token::BorderWidth, TokenValue::Number(0.0)),
        (Token::Warning, TokenValue::Color([30, 40, 50, 255])),
    ]));
    let entity = world.spawn(manual.clone()).id();
    let child = world.spawn(ChildOf(entity)).id();
    set(
        &mut world,
        entity,
        Colors {
            background: Some(Color::Token(Token::Warning)),
            border: Some(Color::Specific([4, 5, 6, 255])),
            ..default()
        },
    );
    assert_eq!(
        crate::token_style::resolve(&world, child, Token::Surface).0,
        TokenValue::Color([30, 40, 50, 255])
    );
    assert_eq!(
        crate::token_style::resolve(&world, entity, Token::BorderWidth).0,
        TokenValue::Number(1.0)
    );
    world
        .get_mut::<crate::tokens::TokenOverrides>(entity)
        .unwrap()
        .0
        .insert(Token::Warning, TokenValue::Color([60, 70, 80, 255]));
    assert_eq!(
        crate::token_style::resolve(&world, child, Token::SandBackground).0,
        TokenValue::Color([60, 70, 80, 255])
    );
    world
        .get_mut::<crate::tokens::TokenOverrides>(entity)
        .unwrap()
        .0
        .remove(&Token::Warning);
    world
        .resource_mut::<crate::tokens::ThemeSettings>()
        .global
        .set(Token::Warning, TokenValue::Color([90, 100, 110, 255]));
    assert_eq!(
        crate::token_style::resolve(&world, child, Token::SandBackground).0,
        TokenValue::Color([90, 100, 110, 255])
    );
    world
        .get_mut::<crate::tokens::TokenOverrides>(entity)
        .unwrap()
        .0
        .insert(Token::BorderWidth, TokenValue::Number(4.0));
    assert_eq!(
        crate::token_style::resolve(&world, entity, Token::BorderWidth).0,
        TokenValue::Number(4.0)
    );
    set(&mut world, entity, Colors::default());
    assert!(world.get::<Colors>(entity).is_none());
    assert_eq!(
        crate::token_style::resolve(&world, entity, Token::SandBackground).0,
        manual.0[&Token::SandBackground]
    );
    let link = world
        .spawn(Colors {
            link: Some(Color::Token(Token::Warning)),
            ..default()
        })
        .id();
    let label = world.spawn(ChildOf(link)).id();
    assert_eq!(
        crate::token_style::resolve(&world, link, Token::Accent).0,
        TokenValue::Color([90, 100, 110, 255])
    );
    assert_eq!(
        crate::token_style::resolve(&world, label, Token::Ink).0,
        TokenValue::Color([90, 100, 110, 255])
    );
}

#[test]
fn data_updates_reapply_card_and_link_colors_without_replacing_entities() {
    let mut world = World::new();
    let owner = world.spawn_empty().id();
    let from = world
        .spawn(crate::protein_area::RecordBinding {
            area: owner,
            uid: "one".into(),
            source: crate::protein_area::Source::Local,
        })
        .id();
    let to = world
        .spawn(crate::protein_area::RecordBinding {
            area: owner,
            uid: "two".into(),
            source: crate::protein_area::Source::Local,
        })
        .id();
    let arrow = world
        .spawn((
            super::super::RelationLink {
                owner,
                uid: "link".into(),
            },
            crate::arrow_sand::ArrowSand {
                from,
                to,
                label: "depends-on".into(),
            },
        ))
        .id();
    let records = HashMap::from([("one".into(), from), ("two".into(), to)]);
    let mut config = crate::relation_castle::config();
    config.relation_styles.rules = vec![
        rule(
            Target::Card,
            vec![quantity(RecordSide::Record, Operator::Greater, "5")],
            Colors {
                background: Some(Color::Token(Token::Warning)),
                ..default()
            },
        ),
        rule(
            Target::Link,
            vec![quantity(RecordSide::Source, Operator::Greater, "5")],
            Colors {
                link: Some(Color::Token(Token::Warning)),
                ..default()
            },
        ),
    ];
    apply(
        &mut world,
        owner,
        &config,
        &[
            json!({"uid":"one","quantity":"6"}),
            json!({"uid":"two","quantity":"2"}),
        ],
        &records,
    );
    assert!(world.get::<Colors>(from).is_some());
    assert!(world.get::<Colors>(to).is_none());
    assert!(world.get::<Colors>(arrow).is_some());
    apply(
        &mut world,
        owner,
        &config,
        &[
            json!({"uid":"one","quantity":"4"}),
            json!({"uid":"two","quantity":"2"}),
        ],
        &records,
    );
    assert!(world.get::<Colors>(from).is_none());
    assert!(world.get::<Colors>(arrow).is_none());
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&config.relation_styles).unwrap()).unwrap();
    assert_eq!(restored, config.relation_styles);
    assert!(config.query().unwrap().include.relation_context.is_some());
}
