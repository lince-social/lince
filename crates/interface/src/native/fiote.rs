use bevy::prelude::*;
pub mod character;
pub mod session;

pub const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "agent-client-protocol",
        author: "Agent Client Protocol Rust SDK contributors",
        license: include_str!("../../licenses/native/agent-client-protocol-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "rmcp",
        author: "Model Context Protocol Rust SDK contributors",
        license: include_str!("../../licenses/native/rmcp-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "openidconnect",
        author: "Ramos Bugs and contributors",
        license: include_str!("../../licenses/native/openidconnect-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "eventsource-stream",
        author: "Julian Popescu and contributors",
        license: include_str!("../../licenses/native/eventsource-stream-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "jcode Retry-After",
        author: "Jeremy Huang",
        license: include_str!("../../licenses/native/jcode-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Argon2",
        author: "RustCrypto developers",
        license: include_str!("../../licenses/native/argon2-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "ChaCha20Poly1305",
        author: "RustCrypto developers",
        license: include_str!("../../licenses/native/chacha20poly1305-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Schemars",
        author: "Graham Esau and contributors",
        license: include_str!("../../licenses/native/schemars-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Loro",
        author: "Loro contributors",
        license: crate::native::credits::LORO_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "reqwest",
        author: "Sean McArthur and contributors",
        license: include_str!("../../licenses/native/reqwest-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "genai",
        author: "Jeremy Chone",
        license: include_str!("../../licenses/native/genai-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "cap-std",
        author: include_str!("../../licenses/native/cap-std-COPYRIGHT.txt"),
        license: include_str!("../../licenses/native/cap-std-MIT.txt"),
    },
    crate::native::credits::SYMBOLS,
    crate::native::credits::DEJAVU,
    crate::native::credits::FONTIQUE,
    crate::native::credits::Attribution {
        name: "Fiote lynx character",
        author: include_str!("../../../../institute/assets/characters/fiote/CREDITS.txt"),
        license: include_str!("../../../../institute/assets/characters/fiote/LICENSE"),
    },
    crate::native::credits::Attribution {
        name: "resvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../../licenses/native/resvg-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "image",
        author: "The image-rs developers",
        license: include_str!("../../licenses/native/image-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::native::credits::LATO_LICENSE,
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
            crate::native::castle::Castle,
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
            crate::native::sand_store::SandCredits(CREDITS),
        ))
        .id();
    world.spawn((
        ImageNode::new(portrait),
        character::Avatar,
        crate::native::sand::Borderless,
        crate::native::sand::Square,
        Outline {
            width: px(0),
            offset: px(2),
            ..default()
        },
        bevy::ui_widgets::Button,
        bevy::input_focus::tab_navigation::TabIndex(0),
        crate::native::actions::ActionButton::new(
            owner,
            crate::native::actions![character::Customize(false)],
        ),
        Node {
            width: px(56),
            height: px(56),
            flex_shrink: 0.0,
            ..default()
        },
        crate::native::icons::Tooltip("Fiote · Customize colors".into()),
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
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
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
        crate::native::token_style::background(crate::native::tokens::Token::Surface),
        Pickable::IGNORE,
        ChildOf(bubble),
    ));
    world.entity_mut(owner).insert(Fiote { bubble });
    owner
}
