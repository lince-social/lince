use super::*;
use crate::actions::Action;
use bevy::text::EditableText;

#[derive(Component)]
pub(super) struct CursorChoice(pub CursorMode);

pub(super) fn origin_label(entry: &Entry, settings: &Settings) -> String {
    if entry.origin["kind"] == "manual" {
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
) -> (Entity, Entity, Entity, Entity, [Entity; 3]) {
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
    chrome::button(world, controls, owner, "Schedule source", SourceNext);
    chrome::button(world, controls, owner, "Theme and tokens", Customize);
    let cursor = crate::sand_panel::row(world, pages[0]);
    for (caption, mode) in [
        ("Cursor moves", CursorMode::Moving),
        ("Cursor stays at top", CursorMode::Fixed),
    ] {
        let button = chrome::button(world, cursor, owner, caption, SetCursor(mode));
        world.entity_mut(button).insert(CursorChoice(mode));
    }
    let panel = crate::sand_panel::row(world, pages[0]);
    let columns: Vec<_> = [48.0, 48.0]
        .into_iter()
        .map(|width| {
            let column = crate::sand_panel::column(world, panel);
            if let Some(mut node) = world.get_mut::<Node>(column) {
                node.width = percent(width);
                node.min_width = px(0);
            }
            column
        })
        .collect();
    let aperture = crate::sand_panel::field(
        world,
        columns[0],
        "Minutes per turn",
        &(settings.aperture_ms as f64 / 60_000.0).to_string(),
    );
    let horizon = crate::sand_panel::field(
        world,
        columns[1],
        "Hours ahead",
        &(settings.horizon_ms as f64 / 3_600_000.0).to_string(),
    );
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
        .id();
    world.entity_mut(viewport).insert(ViewportOwner(owner));
    let details = pages[1];
    (
        viewport,
        status,
        present,
        details,
        [aperture, horizon, timezone],
    )
}

#[derive(Component)]
struct ViewportOwner(Entity);

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
        let Some(horizon) = parse(&values[1], 3_600_000.0) else {
            status(world, owner, "Enter positive future hours within 366 days");
            return;
        };
        let mut settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
        if aperture != settings.aperture_ms {
            settings.set_aperture(aperture);
        }
        if horizon != world.get::<TimeSettings>(owner).unwrap().0.horizon_ms {
            settings.set_horizon(horizon);
        }
        settings.timezone = values[2].clone();
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
            (settings.horizon_ms as f64 / 3_600_000.0).to_string(),
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
struct SourceNext;
impl Action for SourceNext {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
            return;
        };
        let workspace = world
            .get::<crate::workspace::WorkspaceMember>(owner)
            .map(|member| member.0);
        let mut ids: Vec<_> = world
            .query::<(
                &crate::area::InfluenceArea,
                &ChildOf,
                &crate::workspace::WorkspaceMember,
            )>()
            .iter(world)
            .filter(|(area, parent, member)| {
                parent.parent() == root && Some(member.0) == workspace && area.protein.is_some()
            })
            .map(|(area, _, _)| area.id.clone())
            .collect();
        ids.sort();
        let Some(mut settings) = world.get_mut::<TimeSettings>(owner) else {
            return;
        };
        settings.0.area = settings
            .0
            .area
            .as_ref()
            .and_then(|id| ids.iter().position(|next| next == id))
            .map_or_else(|| ids.first().cloned(), |index| ids.get(index + 1).cloned());
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
