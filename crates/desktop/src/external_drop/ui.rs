use super::*;
use crate::actions::Action;
use bevy::{
    picking::{hover::HoverMap, pointer::PointerId},
    ui::InteractionDisabled,
};

#[derive(Component)]
struct ChoicePanel;

#[derive(Clone, Copy)]
pub(super) enum Control {
    Cancel,
    Download,
    Show,
    Editable,
    Link,
}

#[derive(Clone, Copy)]
struct Choice {
    id: u64,
    control: Control,
}

impl Action for Choice {
    fn practice_intent(&self) -> crate::actions::PracticeIntent {
        self.control.practice_intent()
    }
    fn apply(&self, world: &mut World, root: Entity) {
        if world
            .resource::<Sessions>()
            .entries
            .get(&root)
            .is_some_and(|session| session.id == self.id)
        {
            self.control.apply(world, root);
        }
    }
}

impl Action for Control {
    fn practice_intent(&self) -> crate::actions::PracticeIntent {
        if matches!(self, Self::Cancel) {
            crate::actions::PracticeIntent::Recovery
        } else {
            crate::actions::PracticeIntent::Target
        }
    }
    fn apply(&self, world: &mut World, target: Entity) {
        match self {
            Self::Cancel => {
                cancel(world, target);
                world.entity_mut(target).insert(CancelledChoice);
            }
            Self::Download => {
                if world
                    .resource::<Sessions>()
                    .entries
                    .get(&target)
                    .is_some_and(|session| {
                        !session.busy
                            && session.prepared.is_none()
                            && matches!(session.source, source::Source::Url(_))
                    })
                {
                    enqueue(world, target);
                    chooser(world, target);
                }
            }
            Self::Show | Self::Editable | Self::Link => accept(world, target, *self),
        }
    }
}

fn panel(world: &mut World, root: Entity) -> Entity {
    world
        .spawn((
            crate::castle::Castle,
            crate::inspection::InspectionExcluded,
            ChildOf(root),
            GlobalZIndex(200),
            Node {
                position_type: PositionType::Absolute,
                left: px(16),
                top: px(16),
                width: px(440),
                max_width: percent(90),
                max_height: percent(85),
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(14)),
                row_gap: px(8),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .id()
}

pub(super) fn chooser(world: &mut World, root: Entity) {
    let Some(session) = world.resource::<Sessions>().entries.get(&root) else {
        return;
    };
    let hovering = session.hovering;
    let id = session.id;
    let ready = session.ready;
    let (old, name, status, kind, remote, busy, prepared) = (
        session.panel,
        session.source.name(),
        session.status.clone(),
        session.prepared.as_ref().map(|prepared| prepared.kind),
        matches!(session.source, source::Source::Url(_)),
        session.busy,
        session.prepared.is_some(),
    );
    if let Some(old) = old {
        world.despawn(old);
    }
    let owner = panel(world, root);
    world.entity_mut(owner).insert(ChoicePanel);
    crate::edit_mode::label(
        world,
        owner,
        if hovering {
            "Release to choose a Sand"
        } else {
            "Choose a Sand"
        },
        22.0,
    );
    crate::edit_mode::label(world, owner, &name, 14.0);
    crate::edit_mode::label(world, owner, &status, 13.0);
    if hovering {
        world.entity_mut(owner).insert(Pickable::IGNORE);
        world
            .resource_mut::<Sessions>()
            .entries
            .get_mut(&root)
            .unwrap()
            .panel = Some(owner);
        return;
    }
    if let Some(kind) = kind {
        if ready {
            crate::castle_feed::button(
                world,
                owner,
                root,
                &format!("Display as {}", kind.name()),
                Choice {
                    id,
                    control: Control::Show,
                },
            );
        } else {
            crate::edit_mode::label(
                world,
                owner,
                "Loading the actual view before you choose…",
                13.0,
            );
        }
        if kind == source::Kind::Text {
            crate::castle_feed::button(
                world,
                owner,
                root,
                "Use editable text Sand",
                Choice {
                    id,
                    control: Control::Editable,
                },
            );
        }
    }
    if remote {
        crate::castle_feed::button(
            world,
            owner,
            root,
            "Use Link Sand",
            Choice {
                id,
                control: Control::Link,
            },
        );
        if !busy && !prepared {
            crate::castle_feed::button(
                world,
                owner,
                root,
                "Download preview",
                Choice {
                    id,
                    control: Control::Download,
                },
            );
        }
    }
    crate::castle_feed::button(
        world,
        owner,
        root,
        "Cancel",
        Choice {
            id,
            control: Control::Cancel,
        },
    );
    world
        .resource_mut::<Sessions>()
        .entries
        .get_mut(&root)
        .unwrap()
        .panel = Some(owner);
}

pub(super) fn blocked(world: &World, root: Entity, point: Vec2) -> bool {
    world
        .get_resource::<HoverMap>()
        .and_then(|hover| hover.get(&PointerId::Mouse))
        .and_then(|hits| {
            hits.iter()
                .filter(|(entity, _)| world.get_entity(**entity).is_ok())
                .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
        })
        .is_some_and(|(entity, _)| {
            if crate::inspection::bounds(world, *entity)
                .is_some_and(|bounds| !bounds.contains(point))
            {
                return false;
            }
            let mut cursor = Some(*entity);
            while let Some(entity) = cursor {
                if entity == root
                    || world.get::<Preview>(entity).is_some()
                    || world.get::<ChoicePanel>(entity).is_some()
                {
                    return false;
                }
                if world
                    .get::<crate::inspection::InspectionExcluded>(entity)
                    .is_some()
                {
                    return true;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            false
        })
}

pub(super) fn preview(world: &mut World, root: Entity) {
    let mut session = world
        .resource_mut::<Sessions>()
        .entries
        .remove(&root)
        .unwrap();
    let Some(prepared) = &mut session.prepared else {
        world
            .resource_mut::<Sessions>()
            .entries
            .insert(root, session);
        return;
    };
    let owner = match prepared.kind {
        source::Kind::Image => {
            let owner = crate::media_sand::spawn(
                world,
                root,
                session.workspace,
                session.position,
                crate::media_sand::MediaSand::Image {
                    path: prepared.path.to_string_lossy().into_owned(),
                },
            );
            if let Some(pixels) = prepared.pixels.take() {
                crate::media_sand::show(world, owner, pixels);
            }
            owner
        }
        source::Kind::Document => crate::document_viewer::spawn(
            world,
            root,
            session.workspace,
            session.position,
            crate::document_viewer::DocumentViewer::with_path(prepared.path.to_string_lossy()),
        ),
        source::Kind::Model => crate::topology::assets::spawn(
            world,
            root,
            session.workspace,
            session.position,
            prepared.asset.clone().unwrap(),
        ),
        source::Kind::Text => text_sand(
            world,
            root,
            session.workspace,
            session.position,
            prepared.text.as_deref().unwrap_or_default(),
            false,
        ),
    };
    crate::instinct::practice::track_custom(world, root, &[owner]);
    world.entity_mut(owner).insert((
        Preview,
        crate::inspection::InspectionExcluded,
        GlobalZIndex(5),
    ));
    if prepared.kind == source::Kind::Model {
        world.entity_mut(owner).insert((
            crate::topology::assets::CenteredAsset,
            crate::topology::assets::FitOnLoad,
        ));
    }
    let mut placement = crate::topology::spatial(world, owner);
    placement.elevation = session.elevation;
    world.entity_mut(owner).insert(placement);
    session.preview = Some(owner);
    session.ready = matches!(prepared.kind, source::Kind::Image | source::Kind::Text);
    world
        .resource_mut::<Sessions>()
        .entries
        .insert(root, session);
}

fn text_sand(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    text: &str,
    editable: bool,
) -> Entity {
    let kind = if editable {
        crate::sand_store::SandKind::EditableText
    } else {
        crate::sand_store::SandKind::Text
    };
    let owner = crate::sand_store::spawn_sand(world, root, workspace, kind, text, position);
    world
        .get_mut::<crate::canvas::CanvasItem>(owner)
        .unwrap()
        .size = Vec2::new(480.0, 360.0);
    if let Some(content) = world
        .get::<crate::sand_store::StoredSand>(owner)
        .and_then(|sand| sand.content)
        && let Some(mut area) = world.get_mut::<crate::sand_text::SandText>(content)
    {
        area.size = [448.0, 328.0];
    }
    owner
}

fn accept(world: &mut World, root: Entity, control: Control) {
    let Some(session) = world.resource::<Sessions>().entries.get(&root) else {
        return;
    };
    if session.hovering
        || world
            .get::<crate::workspace::Workspaces>(root)
            .is_none_or(|spaces| spaces.active != session.workspace)
    {
        return;
    }
    if matches!(control, Control::Link) && !matches!(session.source, source::Source::Url(_)) {
        return;
    }
    if !matches!(control, Control::Link) && (session.prepared.is_none() || !session.ready) {
        return;
    }
    if matches!(control, Control::Editable)
        && !session
            .prepared
            .as_ref()
            .is_some_and(|prepared| prepared.kind == source::Kind::Text)
    {
        return;
    }
    let mut session = world
        .resource_mut::<Sessions>()
        .entries
        .remove(&root)
        .unwrap();
    session.cancelled.store(true, Ordering::Release);
    let owner = if matches!(control, Control::Link) {
        let source::Source::Url(url) = &session.source else {
            return;
        };
        if let Some(preview) = session.preview.take() {
            world.despawn(preview);
        }
        crate::media_sand::spawn(
            world,
            root,
            session.workspace,
            session.position,
            crate::media_sand::MediaSand::Link { url: url.clone() },
        )
    } else if matches!(control, Control::Editable) {
        if let Some(preview) = session.preview.take() {
            world.despawn(preview);
        }
        text_sand(
            world,
            root,
            session.workspace,
            session.position,
            session.prepared.as_ref().unwrap().text.as_deref().unwrap(),
            true,
        )
    } else if let Some(preview) = session
        .preview
        .take()
        .filter(|entity| world.get_entity(*entity).is_ok())
    {
        restore_tint(world, preview);
        world
            .entity_mut(preview)
            .remove::<(Preview, crate::inspection::InspectionExcluded, GlobalZIndex)>();
        preview
    } else {
        world
            .resource_mut::<Sessions>()
            .entries
            .insert(root, session);
        chooser(world, root);
        return;
    };
    if !matches!(control, Control::Link)
        && let Some(prepared) = &mut session.prepared
    {
        prepared.keep();
    }
    let mut placement = crate::topology::spatial(world, owner);
    placement.elevation = session.elevation;
    world.entity_mut(owner).insert(placement);
    if let Some(panel) = session.panel.take() {
        world.despawn(panel);
    }
}

#[derive(Component)]
struct Tint {
    background: Option<BackgroundColor>,
    text: Option<TextColor>,
    border: Option<BorderColor>,
    image: Option<Color>,
    pickable: Option<Pickable>,
    disabled: bool,
}

#[derive(Component)]
struct MaterialTint(Handle<StandardMaterial>);

#[derive(Component)]
struct SplatTint(f32);

fn descendants(world: &World, owner: Entity) -> Vec<Entity> {
    let mut entities = vec![owner];
    let mut index = 0;
    while index < entities.len() {
        if let Some(children) = world.get::<Children>(entities[index]) {
            entities.extend(children.iter());
        }
        index += 1;
    }
    entities
}

pub(super) fn tint(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Preview>>()
        .iter(world)
        .collect();
    for owner in owners {
        for entity in descendants(world, owner) {
            if world.get::<Node>(entity).is_some() {
                if world.get::<Tint>(entity).is_none() {
                    let tint = Tint {
                        background: world.get::<BackgroundColor>(entity).copied(),
                        text: world.get::<TextColor>(entity).copied(),
                        border: world.get::<BorderColor>(entity).copied(),
                        image: world.get::<ImageNode>(entity).map(|image| image.color),
                        pickable: world.get::<Pickable>(entity).copied(),
                        disabled: world.get::<InteractionDisabled>(entity).is_some(),
                    };
                    world
                        .entity_mut(entity)
                        .insert((tint, Pickable::IGNORE, InteractionDisabled));
                }
                let tint = world.get::<Tint>(entity).unwrap();
                let (background, text, border, image) =
                    (tint.background, tint.text, tint.border, tint.image);
                if let Some(color) = background {
                    world
                        .entity_mut(entity)
                        .insert(BackgroundColor(color.0.with_alpha(color.0.alpha() * 0.45)));
                }
                if let Some(color) = text {
                    world
                        .entity_mut(entity)
                        .insert(TextColor(color.0.with_alpha(color.0.alpha() * 0.45)));
                }
                if let Some(color) = border {
                    world.entity_mut(entity).insert(BorderColor {
                        top: color.top.with_alpha(color.top.alpha() * 0.45),
                        bottom: color.bottom.with_alpha(color.bottom.alpha() * 0.45),
                        left: color.left.with_alpha(color.left.alpha() * 0.45),
                        right: color.right.with_alpha(color.right.alpha() * 0.45),
                    });
                }
                if let Some(color) = image
                    && let Some(mut image) = world.get_mut::<ImageNode>(entity)
                {
                    image.color = color.with_alpha(color.alpha() * 0.45);
                }
            }
            if world.get::<MaterialTint>(entity).is_none()
                && let Some(material) = world
                    .get::<MeshMaterial3d<StandardMaterial>>(entity)
                    .map(|material| material.0.clone())
                && let Some(mut ghost) = world
                    .get_resource::<Assets<StandardMaterial>>()
                    .and_then(|assets| assets.get(&material))
                    .cloned()
            {
                ghost.base_color = ghost.base_color.with_alpha(ghost.base_color.alpha() * 0.45);
                ghost.alpha_mode = AlphaMode::Blend;
                let ghost = world.resource_mut::<Assets<StandardMaterial>>().add(ghost);
                world
                    .entity_mut(entity)
                    .insert((MaterialTint(material), MeshMaterial3d(ghost)));
            }
            if world.get::<SplatTint>(entity).is_none()
                && let Some(opacity) = world
                    .get::<bevy_gaussian_splatting::gaussian::settings::CloudSettings>(entity)
                    .map(|settings| settings.global_opacity)
            {
                world.entity_mut(entity).insert(SplatTint(opacity));
                world
                    .get_mut::<bevy_gaussian_splatting::gaussian::settings::CloudSettings>(entity)
                    .unwrap()
                    .global_opacity = opacity * 0.45;
            }
        }
    }
}

fn restore_tint(world: &mut World, owner: Entity) {
    for entity in descendants(world, owner) {
        if let Some(tint) = world.entity_mut(entity).take::<Tint>() {
            if let Some(color) = tint.background {
                world.entity_mut(entity).insert(color);
            }
            if let Some(color) = tint.text {
                world.entity_mut(entity).insert(color);
            }
            if let Some(color) = tint.border {
                world.entity_mut(entity).insert(color);
            }
            if let Some(color) = tint.image
                && let Some(mut image) = world.get_mut::<ImageNode>(entity)
            {
                image.color = color;
            }
            if let Some(pickable) = tint.pickable {
                world.entity_mut(entity).insert(pickable);
            } else {
                world.entity_mut(entity).remove::<Pickable>();
            }
            if !tint.disabled {
                world.entity_mut(entity).remove::<InteractionDisabled>();
            }
        }
        if let Some(material) = world.entity_mut(entity).take::<MaterialTint>() {
            world.entity_mut(entity).insert(MeshMaterial3d(material.0));
        }
        if let Some(opacity) = world.entity_mut(entity).take::<SplatTint>()
            && let Some(mut settings) =
                world.get_mut::<bevy_gaussian_splatting::gaussian::settings::CloudSettings>(entity)
        {
            settings.global_opacity = opacity.0;
        }
    }
}
