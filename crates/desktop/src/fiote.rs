use bevy::prelude::*;
pub mod character;
pub mod session;

pub const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "agent-client-protocol",
        author: "Agent Client Protocol Rust SDK contributors",
        license: include_str!("../licenses/agent-client-protocol-Apache-2.0.txt"),
    },
    crate::credits::Attribution {
        name: "rmcp",
        author: "Model Context Protocol Rust SDK contributors",
        license: include_str!("../licenses/rmcp-Apache-2.0.txt"),
    },
    crate::credits::Attribution {
        name: "openidconnect",
        author: "Ramos Bugs and contributors",
        license: include_str!("../licenses/openidconnect-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "eventsource-stream",
        author: "Julian Popescu and contributors",
        license: include_str!("../licenses/eventsource-stream-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "jcode Retry-After",
        author: "Jeremy Huang",
        license: include_str!("../licenses/jcode-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "Argon2",
        author: "RustCrypto developers",
        license: include_str!("../licenses/argon2-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "ChaCha20Poly1305",
        author: "RustCrypto developers",
        license: include_str!("../licenses/chacha20poly1305-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "Schemars",
        author: "Graham Esau and contributors",
        license: include_str!("../licenses/schemars-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "Loro",
        author: "Loro contributors",
        license: crate::credits::LORO_LICENSE,
    },
    crate::credits::Attribution {
        name: "reqwest",
        author: "Sean McArthur and contributors",
        license: include_str!("../licenses/reqwest-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "genai",
        author: "Jeremy Chone",
        license: include_str!("../licenses/genai-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "cap-std",
        author: include_str!("../licenses/cap-std-COPYRIGHT.txt"),
        license: include_str!("../licenses/cap-std-MIT.txt"),
    },
    crate::credits::SYMBOLS,
    crate::credits::DEJAVU,
    crate::credits::FONTIQUE,
    crate::credits::Attribution {
        name: "Fiote lynx character",
        author: include_str!("../../../institute/assets/characters/fiote/CREDITS.txt"),
        license: include_str!("../../../institute/assets/characters/fiote/LICENSE"),
    },
    crate::credits::Attribution {
        name: "resvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../licenses/resvg-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "image",
        author: "The image-rs developers",
        license: include_str!("../licenses/image-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::credits::BEVY_LICENSE,
    },
    crate::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::credits::LATO_LICENSE,
    },
];

#[derive(Component)]
pub struct Fiote {
    pub bubble: Entity,
}

pub fn spawn(world: &mut World, parent: Entity) -> Entity {
    let portrait = character::portrait(world, parent);
    let owner = world
        .spawn((
            crate::castle::Castle,
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                bottom: px(16),
                max_width: percent(95),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexEnd,
                column_gap: px(12),
                ..default()
            },
            GlobalZIndex(90),
            ChildOf(parent),
            crate::sand_store::SandCredits(CREDITS),
        ))
        .id();
    world.spawn((
        ImageNode::new(portrait),
        character::Avatar,
        crate::sand::Borderless,
        crate::sand::Square,
        Outline {
            width: px(0),
            offset: px(2),
            ..default()
        },
        bevy::ui_widgets::Button,
        bevy::input_focus::tab_navigation::TabIndex(0),
        crate::actions::ActionButton::new(owner, crate::actions![character::Customize(false)]),
        Node {
            width: px(56),
            height: px(56),
            flex_shrink: 0.0,
            ..default()
        },
        crate::icons::Tooltip("Fiote · Customize colors".into()),
        bevy::a11y::AccessibilityNode({
            let mut node = accesskit::Node::new(accesskit::Role::Button);
            node.set_label("Customize Fiote colors");
            node
        }),
        ChildOf(owner),
    ));
    let bubble = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(16)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(18)),
                min_width: px(0),
                max_width: percent(100),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            crate::token_style::border(crate::tokens::Token::Accent),
            ChildOf(owner),
        ))
        .id();
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(-9),
            bottom: px(20),
            width: px(14),
            height: px(14),
            ..default()
        },
        UiTransform::from_rotation(Rot2::degrees(45.0)),
        crate::token_style::background(crate::tokens::Token::Surface),
        Pickable::IGNORE,
        ChildOf(bubble),
    ));
    world.entity_mut(owner).insert(Fiote { bubble });
    owner
}
