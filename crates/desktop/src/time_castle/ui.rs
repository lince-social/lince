use super::*;
use crate::actions::{Action, ActionButton};
use bevy::text::EditableText;

#[derive(Component)]
pub(super) struct CursorChoice(pub CursorMode);

pub(super) fn origin_label(entry: &Entry, settings: &Settings) -> String {
    if entry.origin["kind"] == "need" {
        "Outstanding quantity on this Record".into()
    } else if entry.origin["kind"] == "manual" {
        "Scheduled on this Record".into()
    } else if entry.origin["cause"]["kind"] == "rule" {
        entry.origin["cause"]["occurrence"]["intended_at_ms"]
            .as_i64()
            .map(|at| format!("Recurring occurrence scheduled for {}", settings.label(at)))
            .unwrap_or_else(|| "Recurring occurrence".into())
    } else {
        "Projected by simulation".into()
    }
}

pub(super) fn time_label(entry: &Entry, settings: &Settings, now: i64) -> String {
    entry.time_label(settings, now)
}

pub(super) fn headline(head: &str) -> String {
    let line = head
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled event");
    let mut chars = line.chars();
    let mut title: String = chars.by_ref().take(56).collect();
    if chars.next().is_some() {
        title.push('…');
    }
    title
}

pub(super) fn populate(
    world: &mut World,
    owner: Entity,
    settings: &Settings,
) -> (Entity, Entity, Entity, Entity, [Entity; 2]) {
    world
        .entity_mut(owner)
        .remove::<(Outline, crate::token_style::OutlineToken)>();
    if let Some(mut node) = world.get_mut::<Node>(owner) {
        node.padding = UiRect::ZERO;
        node.border = UiRect::ZERO;
        node.row_gap = px(0);
        node.overflow = Overflow::visible();
    }
    if let Some(mut background) = world.get_mut::<BackgroundColor>(owner) {
        background.0 = Color::NONE;
    }
    chrome::populate(world, owner);
    let pages = world.get::<chrome::Chrome>(owner).unwrap().pages;
    let controls = crate::sand_panel::row(world, pages[0]);
    chrome::button(world, controls, owner, "Untwist / Coil", Toggle);
    chrome::button(world, controls, owner, "Schedule source", SourceEditor);
    chrome::button(world, controls, owner, "Theme and tokens", Customize);
    let cursor = crate::sand_panel::row(world, pages[0]);
    for (caption, mode) in [
        ("Cursor moves", CursorMode::Moving),
        ("Cursor stays at top", CursorMode::Fixed),
    ] {
        let button = chrome::button(world, cursor, owner, caption, SetCursor(mode));
        world.entity_mut(button).insert(CursorChoice(mode));
    }
    audio::controls(world, owner, pages[3]);
    let aperture = crate::sand_panel::field(
        world,
        pages[0],
        "Aperture (minutes)",
        &(settings.aperture_ms as f64 / 60_000.0).to_string(),
    );
    let cards = chrome::button(
        world,
        pages[0],
        owner,
        card_caption(settings, false),
        ToggleCards,
    );
    world.entity_mut(cards).insert(CardOption(false));
    let physics = chrome::button(
        world,
        pages[0],
        owner,
        card_caption(settings, true),
        TogglePhysics,
    );
    world.entity_mut(physics).insert(CardOption(true));
    let history = chrome::button(
        world,
        pages[0],
        owner,
        if settings.past_tasks {
            "Past tasks: on"
        } else {
            "Past tasks: off"
        },
        TogglePast,
    );
    world.entity_mut(history).insert(PastOption);
    let timezone = crate::sand_panel::field(world, pages[0], "Timezone", &settings.timezone);
    chrome::button(world, pages[0], owner, "Apply settings", Apply);
    let present = crate::edit_mode::label(world, pages[0], "Now", 11.0);
    let status = crate::edit_mode::label(world, pages[0], "Loading schedule", 11.0);
    chrome::button(world, pages[0], owner, "Refresh schedule", Refresh);
    let viewport = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                flex_shrink: 0.0,
                ..default()
            },
            ZIndex(-1),
            ChildOf(owner),
        ))
        .observe(click)
        .observe(hover)
        .id();
    world.entity_mut(viewport).insert(ViewportOwner(owner));
    let details = pages[1];
    (viewport, status, present, details, [aperture, timezone])
}

#[derive(Component)]
struct ViewportOwner(Entity);

#[derive(Component)]
struct UpcomingRow(String);

#[derive(Component)]
struct UpcomingReady;

#[derive(Component)]
struct CardOption(bool);

#[derive(Component)]
struct PastOption;

fn card_caption(settings: &Settings, physics: bool) -> &'static str {
    if physics {
        if settings.card_physics {
            "Card physics: on"
        } else {
            "Card physics: off"
        }
    } else if settings.floating_cards {
        "Cards: floating"
    } else {
        "Cards: on hover"
    }
}

fn set_caption(world: &mut World, button: Entity, caption: &str) {
    let text = world.get::<Children>(button).and_then(|children| {
        children
            .iter()
            .find(|child| world.get::<Text>(*child).is_some())
    });
    if let Some(text) = text {
        world
            .get_mut::<Text>(text)
            .unwrap()
            .set_if_neq(Text::new(caption));
    }
    world.get_mut::<crate::icons::Tooltip>(button).unwrap().0 = caption.into();
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) {
        node.set_label(caption);
    }
}

fn card_options(world: &mut World, owner: Entity) {
    let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
    let buttons: Vec<_> = world
        .query::<(Entity, &CardOption, &crate::actions::ActionButton)>()
        .iter(world)
        .filter(|(_, _, button)| button.target == owner)
        .map(|(entity, option, _)| (entity, option.0))
        .collect();
    for (button, physics) in buttons {
        set_caption(world, button, card_caption(&settings, physics));
    }
}

fn hover(
    event: On<Pointer<Move>>,
    nodes: Query<(&ViewportOwner, &ComputedNode, &UiGlobalTransform)>,
    mut commands: Commands,
) {
    let Ok((owner, node, transform)) = nodes.get(event.entity) else {
        return;
    };
    let Some(inverse) = transform.try_inverse() else {
        return;
    };
    let point = inverse
        .transform_point2(event.pointer_location.position / node.inverse_scale_factor())
        * node.inverse_scale_factor();
    let owner = owner.0;
    commands.queue(move |world: &mut World| {
        let id = render::nearest_at(world, owner, point);
        if let Some(mut view) = world.get_mut::<View>(owner) {
            if id.is_some() {
                view.hovered = id;
                view.hover_point = Some(Vec3::new(point.x, 0.0, point.y));
                view.hover_at = std::time::Instant::now();
            }
        }
    });
}

pub(super) fn upcoming_panel(world: &mut World, owner: Entity) -> Entity {
    let panel = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(28),
                top: percent(36),
                width: percent(44),
                height: percent(34),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            ZIndex(5),
            ChildOf(owner),
        ))
        .id();
    crate::scroll_sand::attach(world, panel);
    world.get_mut::<Node>(panel).unwrap().overflow.x = OverflowAxis::Clip;
    panel
}

pub(super) fn upcoming(
    world: &mut World,
    owner: Entity,
    now: i64,
    visible: bool,
    palette: &palette::Palette,
) {
    let view = world.get::<View>(owner).unwrap();
    let panel = view.upcoming;
    let visible = visible && !chrome::controls_open(world, owner);
    world.get_mut::<Node>(panel).unwrap().display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    if !visible {
        return;
    }
    let view = world.get::<View>(owner).unwrap();
    let settings = &world.get::<TimeSettings>(owner).unwrap().0;
    let stamp = (
        view.revision,
        now.div_euclid(1000),
        settings.aperture_ms,
        settings.timezone.clone(),
    );
    if view.upcoming_stamp.as_ref() == Some(&stamp) {
        return;
    }
    let rows: Vec<_> = model::summaries(&view.entries, now, now + settings.aperture_ms)
        .into_iter()
        .map(|(index, remaining)| {
            let entry = &view.entries[index];
            (
                entry.id.clone(),
                format!(
                    "{}\n{} · {}",
                    headline(&entry.head),
                    entry.time_label(settings, now),
                    model::countdown(remaining)
                ),
            )
        })
        .collect();
    world.get_mut::<View>(owner).unwrap().upcoming_stamp = Some(stamp);
    let view = world.get::<View>(owner).unwrap();
    let ready = world.get::<UpcomingReady>(panel).is_some();
    if ready && view.upcoming_rows == rows {
        return;
    }
    let same_ids = view
        .upcoming_rows
        .iter()
        .map(|(id, _)| id)
        .eq(rows.iter().map(|(id, _)| id));
    if ready && same_ids {
        let buttons: Vec<_> = world
            .query::<(Entity, &UpcomingRow, &ChildOf)>()
            .iter(world)
            .filter(|(_, _, parent)| parent.parent() == panel)
            .map(|(entity, row, _)| (entity, row.0.clone()))
            .collect();
        for (button, id) in buttons {
            if let Some((_, caption)) = rows.iter().find(|(next, _)| *next == id) {
                set_caption(world, button, caption);
            }
        }
        world.get_mut::<View>(owner).unwrap().upcoming_rows = rows;
        return;
    }
    world.entity_mut(panel).despawn_children();
    world.entity_mut(panel).insert(UpcomingReady);
    if rows.is_empty() {
        let heading = crate::edit_mode::label(world, panel, "No upcoming work", 12.0);
        world.get_mut::<TextColor>(heading).unwrap().0 = palette.muted;
        world
            .entity_mut(heading)
            .insert(TextLayout::justify(Justify::Center));
    }
    for (id, text) in &rows {
        let button = chrome::button(world, panel, owner, text, Select(vec![id.clone()]));
        world.entity_mut(button).insert(UpcomingRow(id.clone()));
        let mut node = world.get_mut::<Node>(button).unwrap();
        node.width = percent(100);
        node.min_width = px(0);
        node.flex_shrink = 0.0;
        node.justify_content = JustifyContent::Center;
        let texts: Vec<_> = world
            .get::<Children>(button)
            .unwrap()
            .iter()
            .filter(|child| world.get::<Text>(*child).is_some())
            .collect();
        for text in texts {
            world.entity_mut(text).insert(TextLayout::new(
                Justify::Center,
                bevy::text::LineBreak::WordOrCharacter,
            ));
            let mut node = world.get_mut::<Node>(text).unwrap();
            node.width = percent(100);
            node.min_width = px(0);
            node.flex_shrink = 1.0;
        }
    }
    world.get_mut::<View>(owner).unwrap().upcoming_rows = rows;
}

#[derive(Clone)]
struct ToggleCards;

impl Action for ToggleCards {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut settings) = world.get_mut::<TimeSettings>(owner) {
            settings.0.floating_cards = !settings.0.floating_cards;
            let message = if settings.0.floating_cards {
                "Floating cards enabled"
            } else {
                "Cards appear when hovering a point or range"
            };
            status(world, owner, message);
        }
        card_options(world, owner);
    }
}

#[derive(Clone)]
struct TogglePhysics;

impl Action for TogglePhysics {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut settings) = world.get_mut::<TimeSettings>(owner) {
            settings.0.card_physics = !settings.0.card_physics;
            let message = if settings.0.card_physics {
                "Card physics enabled"
            } else {
                "Card physics disabled"
            };
            status(world, owner, message);
        }
        card_options(world, owner);
    }
}

#[derive(Clone)]
struct TogglePast;

impl Action for TogglePast {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut settings) = world.get_mut::<TimeSettings>(owner) else {
            return;
        };
        settings.0.past_tasks = !settings.0.past_tasks;
        let caption = if settings.0.past_tasks {
            "Past tasks: on"
        } else {
            "Past tasks: off"
        };
        let buttons: Vec<_> = world
            .query::<(Entity, &PastOption, &ActionButton)>()
            .iter(world)
            .filter(|(_, _, action)| action.target == owner)
            .map(|(button, _, _)| button)
            .collect();
        for button in buttons {
            set_caption(world, button, caption);
        }
    }
}

fn click(
    mut event: On<Pointer<Click>>,
    nodes: Query<(&ViewportOwner, &ComputedNode, &UiGlobalTransform)>,
    mut commands: Commands,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    let Ok((owner, node, transform)) = nodes.get(event.entity) else {
        return;
    };
    let Some(inverse) = transform.try_inverse() else {
        return;
    };
    let point = inverse
        .transform_point2(event.pointer_location.position / node.inverse_scale_factor())
        * node.inverse_scale_factor();
    let owner = owner.0;
    commands.queue(move |world: &mut World| render::select_at(world, owner, point));
    event.propagate(false);
}

#[derive(Clone)]
struct Toggle;
impl Action for Toggle {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut settings) = world.get_mut::<TimeSettings>(owner) {
            settings.0.mode = if settings.0.mode == Mode::Coiled {
                Mode::Straight
            } else {
                Mode::Coiled
            };
        }
        if let Some(mut view) = world.get_mut::<View>(owner) {
            view.animation_at = std::time::Instant::now();
        }
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

#[derive(Clone)]
struct SetCursor(CursorMode);

impl Action for SetCursor {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut settings) = world.get_mut::<TimeSettings>(owner) {
            settings.0.cursor = self.0;
        }
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}

#[derive(Clone)]
struct Customize;

impl Action for Customize {
    fn apply(&self, world: &mut World, owner: Entity) {
        let mut ancestor = world.get::<ChildOf>(owner).map(ChildOf::parent);
        while let Some(entity) = ancestor {
            if world.get::<crate::edit_mode::EditMode>(entity).is_some() {
                world
                    .entity_mut(entity)
                    .insert(crate::customization::Scope::Sand(owner));
                crate::edit_mode::show_customization(world, entity);
                return;
            }
            ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
}

#[derive(Clone)]
struct Apply;
impl Action for Apply {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let values: Vec<_> = view
            .fields
            .iter()
            .map(|field| {
                world
                    .get::<EditableText>(*field)
                    .map(|text| text.value().to_string().trim().to_owned())
                    .unwrap_or_default()
            })
            .collect();
        let parse = |value: &str, multiplier: f64| -> Option<i64> {
            let number = value.parse::<f64>().ok()? * multiplier;
            (number.is_finite() && number >= 1000.0 && number <= model::MAX_HORIZON_MS as f64)
                .then_some(number.round() as i64)
        };
        let Some(aperture) = parse(&values[0], 60_000.0) else {
            status(world, owner, "Enter positive minutes within 366 days");
            return;
        };
        let mut settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
        settings.aperture_ms = aperture;
        settings.horizon_ms = aperture;
        settings.timezone = values[1].clone();
        if !settings.valid() {
            status(
                world,
                owner,
                "Enter a valid IANA timezone, such as America/Sao_Paulo",
            );
            return;
        }
        world.get_mut::<TimeSettings>(owner).unwrap().0 = settings.clone();
        let fields = world.get::<View>(owner).unwrap().fields;
        let values = [
            (settings.aperture_ms as f64 / 60_000.0).to_string(),
            settings.timezone,
        ];
        for (field, value) in fields.into_iter().zip(values) {
            if let Some(mut text) = world.get_mut::<EditableText>(field) {
                text.editor.set_text(&value);
            }
        }
        world.get_mut::<View>(owner).unwrap().fallback = false;
        status(world, owner, "Clock settings saved");
    }
}

#[derive(Clone)]
struct SourceEditor;
impl Action for SourceEditor {
    fn apply(&self, world: &mut World, owner: Entity) {
        source::open(world, owner);
    }
}

#[derive(Clone)]
struct Refresh;
impl Action for Refresh {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Ok((Source::Organ(organ), _)) = source_query(world, owner) {
            crate::protein_area::reconnect_auxiliary(world, &organ);
        }
        if let Some(feed) = world.resource_mut::<Feeds>().active.get_mut(&owner) {
            feed.sent = false;
        }
        status(world, owner, "Refreshing schedule source");
    }
}

#[derive(Clone)]
pub(super) struct Select(pub Vec<String>);
impl Action for Select {
    fn apply(&self, world: &mut World, owner: Entity) {
        chrome::reveal(world, owner);
        let Some(mut view) = world.get_mut::<View>(owner) else {
            return;
        };
        view.selected = self.0.clone();
        view.revision = view.revision.wrapping_add(1);
        view.page = 0;
        view.detail_revision = u64::MAX;
        if self.0.len() == 1 {
            let entry = view
                .entries
                .iter()
                .find(|entry| entry.id == self.0[0])
                .cloned();
            let source = view.source.clone();
            let timezone = world.get::<TimeSettings>(owner).unwrap().0.timezone.clone();
            if let Some(entry) = entry {
                let selection = model::Selection {
                    record_uid: entry.record_uid.clone(),
                    entry,
                    source,
                    timezone,
                };
                if let Ok(value) = serde_json::to_value(selection) {
                    crate::scoped_events::emit(world, owner, model::RECORD_SELECTED, value);
                }
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct Page(pub(super) i32);
impl Action for Page {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut view) = world.get_mut::<View>(owner) {
            view.page = view.page.saturating_add_signed(self.0 as isize);
            view.detail_revision = u64::MAX;
        }
    }
}

pub(super) fn details(world: &mut World, owner: Entity, now: i64) {
    let view = world.get::<View>(owner).unwrap();
    if view.detail_revision == view.revision {
        return;
    }
    let (parent, selected, page) = (view.details, view.selected.clone(), view.page);
    let selected_ids: std::collections::HashSet<_> = selected.iter().collect();
    let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
    let mut entries: Vec<_> = view
        .entries
        .iter()
        .filter(|entry| {
            if !selected.is_empty() {
                selected_ids.contains(&entry.id)
            } else {
                entry.category_at(now, &settings.timezone) != model::Category::Timed
                    || entry
                        .time
                        .as_ref()
                        .is_some_and(|time| time.overlaps(now, now + settings.horizon_ms))
            }
        })
        .cloned()
        .collect();
    if selected.is_empty() {
        let rank = |entry: &Entry| match entry.category_at(now, &settings.timezone) {
            model::Category::Overdue => 0,
            model::Category::AllDay => 1,
            model::Category::Timed => 2,
        };
        entries.sort_by(|a, b| {
            rank(a)
                .cmp(&rank(b))
                .then_with(|| {
                    a.time
                        .as_ref()
                        .map(|time| time.from_ms)
                        .cmp(&b.time.as_ref().map(|time| time.from_ms))
                })
                .then_with(|| a.id.cmp(&b.id))
        });
    }
    let pages = entries.len().div_ceil(20).max(1);
    let page = page.min(pages - 1);
    world.entity_mut(parent).despawn_children();
    let controls = crate::sand_panel::row(world, parent);
    chrome::button(
        world,
        controls,
        owner,
        "All scheduled work",
        Select(Vec::new()),
    );
    if pages > 1 {
        chrome::button(world, controls, owner, "Previous", Page(-1));
        chrome::button(world, controls, owner, "Next", Page(1));
    }
    crate::edit_mode::label(
        world,
        parent,
        &format!(
            "{} occurrence(s) · page {} / {pages}",
            entries.len(),
            page + 1
        ),
        12.0,
    );
    let mut previous_category = None;
    for entry in entries.iter().skip(page * 20).take(20) {
        let current_category = entry.category_at(now, &settings.timezone);
        if previous_category != Some(current_category) {
            crate::edit_mode::label(
                world,
                parent,
                match current_category {
                    model::Category::Timed => "Future schedule",
                    model::Category::AllDay => "All-day work",
                    model::Category::Overdue => "Overdue work",
                },
                15.0,
            );
            previous_category = Some(current_category);
        }
        let category = match entry.category_at(now, &settings.timezone) {
            model::Category::Timed => "Scheduled",
            model::Category::AllDay => "All day",
            model::Category::Overdue => "Overdue",
        };
        let time = time_label(entry, &settings, now);
        let event = chrome::button(
            world,
            parent,
            owner,
            &format!(
                "{category} · {}\n{time}{}",
                headline(&entry.head),
                if entry.preview {
                    " · projection preview (read only)"
                } else {
                    ""
                }
            ),
            Select(vec![entry.id.clone()]),
        );
        world.get_mut::<Node>(event).unwrap().width = percent(100);
        if !selected.is_empty() {
            let origin = origin_label(entry, &settings);
            crate::edit_mode::label(world, parent, &origin, 11.0);
        }
    }
    if entries.is_empty() {
        crate::edit_mode::label(world, parent, "No scheduled work in this window", 12.0);
    }
    let peek_entries = if selected.is_empty() {
        &[][..]
    } else {
        &entries[..]
    };
    chrome::peek(world, owner, peek_entries, &settings);
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.page = page;
    view.detail_revision = view.revision;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(crate::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let owner = world
            .spawn((
                Node::default(),
                TimeSettings(Settings {
                    horizon_ms: 7_200_000,
                    ..Settings::default()
                }),
            ))
            .id();
        super::super::populate(&mut world, owner);
        (world, owner)
    }

    #[test]
    fn aperture_is_the_only_window_field_and_toggles_are_saved_in_settings() {
        let (mut world, owner) = fixture();
        let settings = &world.get::<TimeSettings>(owner).unwrap().0;
        assert_eq!(settings.horizon_ms, settings.aperture_ms);
        let fields = world.get::<View>(owner).unwrap().fields;
        world
            .get_mut::<EditableText>(fields[0])
            .unwrap()
            .editor
            .set_text("120");
        world
            .get_mut::<EditableText>(fields[1])
            .unwrap()
            .editor
            .set_text("America/Sao_Paulo");
        Apply.apply(&mut world, owner);
        let settings = &world.get::<TimeSettings>(owner).unwrap().0;
        assert_eq!(settings.aperture_ms, 7_200_000);
        assert_eq!(settings.horizon_ms, settings.aperture_ms);
        assert_eq!(settings.timezone, "America/Sao_Paulo");
        ToggleCards.apply(&mut world, owner);
        TogglePhysics.apply(&mut world, owner);
        assert!(world.get::<TimeSettings>(owner).unwrap().0.past_tasks);
        TogglePast.apply(&mut world, owner);
        let settings = &world.get::<TimeSettings>(owner).unwrap().0;
        let restored: Settings =
            serde_json::from_value(serde_json::to_value(settings).unwrap()).unwrap();
        assert!(!restored.floating_cards);
        assert!(!restored.past_tasks);
        assert!(!restored.card_physics);
        world
            .get_mut::<EditableText>(fields[0])
            .unwrap()
            .editor
            .set_text("NaN");
        Apply.apply(&mut world, owner);
        assert_eq!(world.get::<TimeSettings>(owner).unwrap().0, restored);
        world
            .get_mut::<EditableText>(fields[0])
            .unwrap()
            .editor
            .set_text("120");
        world
            .get_mut::<EditableText>(fields[1])
            .unwrap()
            .editor
            .set_text("Unknown/Zone");
        Apply.apply(&mut world, owner);
        assert_eq!(world.get::<TimeSettings>(owner).unwrap().0, restored);
    }

    #[test]
    fn upcoming_list_keeps_all_aperture_entries_and_scroll_position_during_countdowns() {
        let (mut world, owner) = fixture();
        let entries: Vec<Entry> = (0..8).map(|index| serde_json::from_value(serde_json::json!({
            "uid": format!("task-{index}"), "record_uid": format!("record-{index}"),
            "head": format!("Task {index}"), "quantity": "-1", "category": "timed",
            "time": {"from_ms": 10_000 + index * 1000}, "origin": {"kind": "manual"}, "preview": false
        })).unwrap()).collect();
        world.get_mut::<View>(owner).unwrap().entries = entries;
        let palette = palette::Palette::resolve(&world, owner);
        upcoming(&mut world, owner, 1000, true, &palette);
        let panel = world.get::<View>(owner).unwrap().upcoming;
        assert_eq!(world.get::<View>(owner).unwrap().upcoming_rows.len(), 8);
        assert_eq!(
            world.get::<Node>(panel).unwrap().overflow.y,
            OverflowAxis::Scroll
        );
        assert_eq!(
            world.get::<Node>(panel).unwrap().overflow.x,
            OverflowAxis::Clip
        );
        let children = world.get::<Children>(panel).unwrap().to_vec();
        assert_eq!(children.len(), 8);
        for child in &children {
            let texts = world.get::<Children>(*child).unwrap();
            for text in texts
                .iter()
                .filter(|entity| world.get::<Text>(*entity).is_some())
            {
                assert_eq!(
                    world.get::<TextLayout>(text).unwrap().justify,
                    Justify::Center
                );
                assert!(!world.get::<Text>(text).unwrap().0.contains("Next "));
            }
        }
        world.get_mut::<ScrollPosition>(panel).unwrap().0.y = 40.0;
        upcoming(&mut world, owner, 2000, true, &palette);
        assert_eq!(world.get::<Children>(panel).unwrap().to_vec(), children);
        assert_eq!(world.get::<ScrollPosition>(panel).unwrap().0.y, 40.0);
        upcoming(&mut world, owner, 2000, false, &palette);
        assert_eq!(world.get::<Node>(panel).unwrap().display, Display::None);
    }
}
