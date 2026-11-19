use crate::native::{
    actions::Action,
    layout::desktop::input_settings,
    protein_area::{RecordBinding, Source},
    workspace::Workspaces,
};
use bevy::{prelude::*, text::EditableText};

mod attachments;
mod presentation;
pub use attachments::{Attachment, attach, detach};

#[derive(Component)]
struct Page {
    root: Entity,
    canvas: String,
    target: Entity,
    input: Entity,
    summary: Entity,
    status: Entity,
    detach: Entity,
    open: Entity,
    message: String,
}

#[derive(Component)]
struct Inspector {
    root: Entity,
    target: Entity,
}

#[derive(Clone, Copy)]
enum Change {
    Attach,
    Detach,
    Open,
    Close,
}

#[derive(Clone, Copy)]
pub struct Open;

impl Action for Open {
    fn apply(&self, world: &mut World, item: Entity) {
        if let Some(root) = crate::native::layout::desktop::root(world, item) {
            open(world, root, item);
        }
    }
}

pub struct ItemPropertiesPlugin;

impl Plugin for ItemPropertiesPlugin {
    fn build(&self, app: &mut App) {
        attachments::install(app);
        app.add_systems(
            PostUpdate,
            refresh
                .after(crate::native::actions::ApplyActions)
                .before(bevy::ui::UiSystems::Layout),
        );
    }
}

fn eligible(world: &World, root: Entity, item: Entity) -> bool {
    attachments::source_allowed(world, item)
        && crate::native::canvas_item::eligible(world, root, item)
        && world
            .get_resource::<crate::host::input::InputActivity>()
            .is_none_or(|activity| activity.active())
        && !crate::native::laboratory::blocked(world, root)
}

pub fn open(world: &mut World, root: Entity, target: Entity) -> Option<Entity> {
    if !eligible(world, root, target) {
        return None;
    }
    let old: Vec<_> = world
        .query::<(Entity, &Inspector)>()
        .iter(world)
        .filter(|(_, inspector)| inspector.root == root)
        .map(|(entity, _)| entity)
        .collect();
    for entity in old {
        world.despawn(entity);
    }
    let owner = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(56),
                width: px(420),
                max_width: percent(95),
                max_height: percent(85),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(12)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            GlobalZIndex(30),
            crate::native::inspection::InspectionExcluded,
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            Inspector { root, target },
            ChildOf(root),
            Name::new("item.properties"),
        ))
        .id();
    crate::native::scroll_sand::decorate(world, owner);
    crate::native::edit_mode::label(world, owner, "Item properties", 20.0);
    input_settings::button(world, owner, owner, "Close properties", Change::Close);
    panel(world, root, owner, target);
    if let Some(metadata) = metadata_change(world, target, None) {
        world.entity_mut(owner).insert(metadata);
        crate::native::full_record::window::populate(
            world,
            owner,
            &RecordBinding {
                area: owner,
                uid: String::new(),
                source: Source::Local,
            },
        );
    }
    Some(owner)
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity, target: Entity) {
    if world
        .get::<crate::native::canvas::CanvasItem>(target)
        .is_none()
    {
        return;
    }
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return;
    };
    let canvas = spaces.canvas_id.clone();
    let page = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
            Name::new("item.record-properties"),
        ))
        .id();
    let summary = crate::native::edit_mode::label(world, page, "Record properties", 16.0);
    let input = input_settings::field(world, page, "Record UID", 128);
    if let Some(attachment) = world.get::<Attachment>(target).cloned() {
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text(&attachment.uid);
    }
    input_settings::button(
        world,
        page,
        page,
        "Attach Record properties",
        Change::Attach,
    );
    let detach = input_settings::button(
        world,
        page,
        page,
        "Detach Record properties",
        Change::Detach,
    )
    .0;
    let open = input_settings::button(world, page, page, "Open attached Record", Change::Open).0;
    let status = input_settings::feedback(world, page);
    world.entity_mut(page).insert(Page {
        root,
        canvas,
        target,
        input,
        summary,
        status,
        detach,
        open,
        message: String::new(),
    });
}

impl Action for Change {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Close) {
            if world.get::<Inspector>(owner).is_some() {
                world.despawn(owner);
            }
            return;
        }
        let Some(page) = world.get::<Page>(owner) else {
            return;
        };
        if crate::native::shortcuts::keyboard_owned(world)
            || !crate::native::shortcuts::input_eligible(world, page.root, &page.canvas, owner)
        {
            return;
        }
        let (root, target, input) = (page.root, page.target, page.input);
        let result = match self {
            Self::Attach => {
                let Some(input) = world
                    .get::<EditableText>(input)
                    .filter(|input| !input.is_composing() && input.pending_paste.is_none())
                else {
                    return;
                };
                let uid = input.value().to_string();
                attach(world, root, target, &uid)
            }
            Self::Detach => detach(world, root, target),
            Self::Open => {
                if !eligible(world, root, target) {
                    Err("The original canvas item is unavailable.")
                } else if let Some(attachment) = world.get::<Attachment>(target).cloned() {
                    crate::native::full_record::open(world, root, &attachment.uid, Source::Local);
                    Ok(())
                } else {
                    Err("No Record is attached.")
                }
            }
            Self::Close => unreachable!(),
        };
        world.get_mut::<Page>(owner).unwrap().message = result.err().unwrap_or_default().into();
    }
}

fn metadata_change(
    world: &World,
    target: Entity,
    current: Option<&crate::native::full_record::window::Metadata>,
) -> Option<crate::native::full_record::window::Metadata> {
    world.get::<crate::native::layout::desktop::ApplicationWindow>(target)?;
    let placement = world.get::<crate::native::layout::LayoutBox>(target)?.id;
    let properties = world
        .get::<crate::native::area::RecordProperties>(target)
        .map(|properties| &properties.0);
    let title = properties
        .and_then(|properties| {
            properties
                .get("window_title")
                .or_else(|| properties.get("head"))
        })
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let app_id = properties
        .and_then(|properties| properties["app_id"].as_str())
        .unwrap_or_default();
    if current.is_some_and(|current| {
        current.placement == placement
            && current.uid.is_empty()
            && current.title == title
            && current.app_id == app_id
            && current.live
            && current.state == "available"
    }) {
        return None;
    }
    crate::host::performance::count(crate::host::performance::Count::ItemPropertyMetadataPrepared);
    Some(crate::native::full_record::window::Metadata {
        placement,
        uid: String::new(),
        title: title.to_owned(),
        app_id: app_id.to_owned(),
        live: true,
        state: "available".into(),
    })
}

fn refresh(world: &mut World, mut prepared: Local<presentation::Refresh>) {
    prepared.refresh(world);
}

#[cfg(test)]
mod tests;
