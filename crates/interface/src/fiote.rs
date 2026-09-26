use bevy::prelude::*;
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
        name: "ureq",
        author: "The ureq developers",
        license: include_str!("../licenses/ureq-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "modelbridge",
        author: "Ryan Sayer",
        license: include_str!("../licenses/modelbridge-MIT.txt"),
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
        name: "Lince logo",
        author: "Lince",
        license: include_str!("../../../LICENSE"),
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

#[derive(Resource)]
struct Logo(Handle<Image>);

pub fn spawn(world: &mut World, parent: Entity) -> Entity {
    if !world.contains_resource::<Logo>() {
        world.init_resource::<Assets<Image>>();
        let image =
            image::load_from_memory(include_bytes!("../../../institute/assets/logo/black_in_white.png"))
                .expect("Lince logo")
                .to_rgba8();
        let handle = world.resource_mut::<Assets<Image>>().add(Image::new(
            bevy::render::render_resource::Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            image.into_raw(),
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::RENDER_WORLD,
        ));
        world.insert_resource(Logo(handle));
    }
    let logo = world.resource::<Logo>().0.clone();
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
        ImageNode::new(logo),
        Node {
            width: px(56),
            height: px(56),
            flex_shrink: 0.0,
            ..default()
        },
        crate::icons::Tooltip("Fiote".into()),
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
