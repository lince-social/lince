use super::*;

const ACTIONS: [(DesktopAction, &str); 8] = [
    (DesktopAction::CreateCompactGroup, "Create a compact group"),
    (DesktopAction::AddCompactMembers, "Add compact members"),
    (DesktopAction::PreviousCompactMember, "Select previous member"),
    (DesktopAction::NextCompactMember, "Select next member"),
    (DesktopAction::MoveCompactMember(crate::host::Direction::Left), "Move member earlier"),
    (DesktopAction::MoveCompactMember(crate::host::Direction::Right), "Move member later"),
    (DesktopAction::DetachCompactMember, "Remove this item from group"),
    (DesktopAction::DissolveCompactGroup, "Ungroup all members"),
];

#[derive(Component)]
struct Panel {
    target: Target,
    summary: Entity,
    status: Entity,
    buttons: Vec<(DesktopAction, Entity)>,
}

fn panel(world: &mut World, parent: Entity, target: Target) {
    let page = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), flex_shrink: 0.0, ..default() }, ChildOf(parent), Name::new("item.compact-properties"))).id();
    let summary = crate::native::edit_mode::label(world, page, "Compact group", 16.0);
    world.get_mut::<Node>(summary).unwrap().width = percent(100);
    world.entity_mut(summary).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    let mut buttons = Vec::new();
    for (action, caption) in ACTIONS {
        let button = input_settings::button(world, page, page, caption, Change::Action(target.clone(), action)).0;
        buttons.push((action, button));
    }
    let status = input_settings::feedback(world, page);
    world.entity_mut(page).insert(Panel { target, summary, status, buttons });
}

pub(crate) fn item_panel(world: &mut World, root: Entity, parent: Entity, entity: Entity) {
    match capture(world, root, entity, false) {
        Ok(target) => panel(world, parent, target),
        Err(_) => { crate::native::edit_mode::label(world, parent, "Compact controls become available for managed items in tiling.", 14.0); }
    }
}

pub(crate) fn window_panel(world: &mut World, parent: Entity, binding: &RecordBinding) {
    let Some(metadata) = world.get::<Metadata>(binding.area).filter(|metadata| metadata.uid == binding.uid).cloned() else { return; };
    let Some(root) = root(world, binding.area) else { return; };
    let target = world.get::<Children>(root).and_then(|children| children.iter().find(|entity|
        world.get::<LayoutBox>(*entity).is_some_and(|layout| layout.id == metadata.placement) && world.get::<ApplicationWindow>(*entity).is_some()));
    let Some(mut target) = target.and_then(|entity| capture(world, root, entity, false).ok()) else {
        crate::native::edit_mode::label(world, parent, "Compact controls require the original live, visible application window.", 14.0); return;
    };
    target.inspector = Some((binding.area, binding.uid.clone()));
    panel(world, parent, target);
}

pub(super) fn refresh(world: &mut World) {
    let pages: Vec<_> = world.query_filtered::<Entity, With<Panel>>().iter(world).collect();
    for page in pages {
        let state = world.get::<Panel>(page).unwrap();
        let source = model(world, &state.target, false);
        let group = source.as_ref().ok().and_then(|model| model.compact_group(state.target.placement));
        let summary = if source.is_err() { "Original item unavailable · compact grouping".to_owned() } else { group.map_or_else(|| "Single item · compact grouping".to_owned(), |group| {
            let index = group.members.iter().position(|id| *id == group.active).unwrap_or(0) + 1;
            format!("Compact group · member {index} of {}", group.members.len())
        }) };
        let status = source.as_ref().err().copied().unwrap_or_default();
        let fields = (state.summary, state.status);
        let buttons = state.buttons.iter().map(|(action, button)| {
            let show = if *action == DesktopAction::CreateCompactGroup { group.is_none() } else { group.is_some() };
            (*button, show, unavailable(world, &state.target, *action))
        }).collect::<Vec<_>>();
        input_settings::text(world, fields.0, summary);
        input_settings::text(world, fields.1, status);
        for (button, show, reason) in buttons {
            let display = if show { Display::Flex } else { Display::None };
            if let Some(mut node) = world.get_mut::<Node>(button) && node.display != display { node.display = display; }
            input_settings::disable(world, button, reason.is_some());
            if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) && node.description() != reason {
                if let Some(reason) = reason { node.set_description(reason); } else { node.clear_description(); }
            }
        }
    }
}
