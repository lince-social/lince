use super::*;
use crate::host::capture::{Session, Status, Target};

#[derive(Component)]
struct Page {
    body: Entity,
    summary: Entity,
    rows: BTreeMap<u64, (Entity, Entity)>,
}

#[derive(Component)]
struct Indicator { root: Entity, label: Entity }

#[derive(Clone, Copy)]
struct Stop { id: u64, connection: u64 }

impl Action for Stop {
    fn apply(&self, world: &mut World, _: Entity) {
        if crate::native::shortcuts::keyboard_owned(world) { return; }
        let Some(status) = world.get_resource::<Status>().filter(|status| status.available && !status.paused) else { return; };
        if !status.sessions.iter().any(|session| session.id == self.id && session.connection == self.connection) { return; }
        let sent = world.get_resource::<crate::native::layout::desktop::HostRequests>()
            .is_some_and(|requests| requests.0.try_send(crate::host::HostRequest::StopCapture(self.id)).is_ok());
        if !sent { crate::native::notifications::report(world, "External capture", "The host is busy or unavailable. Retry stopping the capture session."); }
        if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() { wake.ring(); }
    }
}

fn caption(session: &Session) -> String {
    let kind = match session.target { Target::Output(_) => "Output", Target::Window(_) => "Window" };
    let label: String = session.label.chars().filter(|character| !character.is_control()).take(512).collect();
    format!("{kind}: {label} · authorized connection {}{}{}", session.connection, if session.cursor { " · cursor stream" } else { "" }, if session.paused { " · waiting for an available privacy-safe frame" } else { "" })
}

fn summary(status: &Status) -> String {
    if !status.available { return "Global capture sessions are available in Park. Sandbox cannot observe its parent compositor's capture sessions.".into(); }
    if status.sessions.is_empty() { return "No active external capture sessions. Your selected screenshot/portal service owns consent and recording.".into(); }
    format!("{} external capture session{}{}. This reports capture targets, not whether another program is recording to disk.", status.sessions.len(), if status.sessions.len() == 1 { "" } else { "s" }, if status.paused { " · paused while the view is inactive" } else { "" })
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    let page = world.spawn((Node { width: percent(100), min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }, ChildOf(parent))).id();
    crate::native::edit_mode::label(world, page, "External capture and sharing", 20.0);
    let summary = crate::native::edit_mode::label(world, page, "", 14.0);
    world.get_mut::<Node>(summary).unwrap().width = percent(100);
    world.entity_mut(summary).insert(TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter));
    let body = world.spawn((Node { width: percent(100), min_width: px(0), max_height: px(240), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }, ChildOf(page))).id();
    crate::native::scroll_sand::attach(world, body);
    world.entity_mut(page).insert(Page { body, summary, rows: BTreeMap::new() });
}

pub(super) fn refresh(world: &mut World) {
    let status = world.resource::<Status>().clone();
    let pages: Vec<_> = world.query_filtered::<Entity, With<Page>>().iter(world).collect();
    for page in pages {
        let summary_label = world.get::<Page>(page).unwrap().summary;
        input_settings::text(world, summary_label, summary(&status));
        let retired: Vec<_> = world.get::<Page>(page).unwrap().rows.keys().filter(|id| !status.sessions.iter().any(|session| session.id == **id)).copied().collect();
        for id in retired {
            let (row, _) = world.get_mut::<Page>(page).unwrap().rows.remove(&id).unwrap();
            world.despawn(row);
        }
        let body = world.get::<Page>(page).unwrap().body;
        for session in &status.sessions {
            let (button, label) = if let Some(row) = world.get::<Page>(page).unwrap().rows.get(&session.id) { *row } else {
                let row = input_settings::button(world, body, page, &caption(session), Stop { id: session.id, connection: session.connection });
                if let Target::Window(surface) = session.target { world.entity_mut(row.0).insert(WindowSources(vec![surface])); }
                world.get_mut::<Page>(page).unwrap().rows.insert(session.id, row);
                row
            };
            input_settings::text(world, label, format!("{} · Stop capture", caption(session)));
            input_settings::disable(world, button, status.paused);
            if let Some(mut accessible) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) { accessible.set_label(format!("Stop capture: {}", caption(session))); }
        }
    }
    let roots: Vec<_> = world.query_filtered::<Entity, (With<Workspaces>, With<crate::native::canvas::CanvasView>)>().iter(world).collect();
    for root in roots {
        let current = world.query::<(Entity, &Indicator)>().iter(world).find(|(_, indicator)| indicator.root == root).map(|(entity, indicator)| (entity, indicator.label));
        if status.sessions.is_empty() {
            if let Some((entity, _)) = current { world.despawn(entity); }
            continue;
        }
        let (_, label) = current.unwrap_or_else(|| {
            let entity = world.spawn((
                Node { position_type: PositionType::Absolute, left: px(12), top: px(12), max_width: percent(80), padding: UiRect::all(px(8)), ..default() },
                crate::native::token_style::background(crate::native::tokens::Token::Surface),
                crate::native::inspection::InspectionExcluded,
                bevy::picking::Pickable::IGNORE,
                GlobalZIndex(20),
                ChildOf(root),
                Name::new("capture.active-indicator"),
            )).id();
            let label = crate::native::edit_mode::label(world, entity, "", 14.0);
            world.entity_mut(label).insert((bevy::picking::Pickable::IGNORE, TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter)));
            crate::native::accessibility::status(world, label);
            world.entity_mut(entity).insert(Indicator { root, label });
            (entity, label)
        });
        let targets = status.sessions.iter().take(3).map(caption).collect::<Vec<_>>().join("; ");
        let window_sources = status.sessions.iter().filter_map(|session| match session.target { Target::Window(surface) => Some(surface), Target::Output(_) => None }).collect();
        world.entity_mut(label).insert(WindowSources(window_sources));
        let text = format!("{}: {targets}{}", if status.paused || status.sessions.iter().all(|session| session.paused) { "Capture paused" } else { "Sharing" }, if status.sessions.len() > 3 { "… · details in Edit → Park" } else { "" });
        input_settings::text(world, label, &text);
    }
}

#[cfg(test)]
mod tests;
