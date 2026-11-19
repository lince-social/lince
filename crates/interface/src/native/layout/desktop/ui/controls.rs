use super::*;
use crate::native::shortcuts::{Settings, Shortcut};
use std::{collections::HashSet, sync::LazyLock};

static DEFAULTS: LazyLock<Settings> = LazyLock::new(Settings::default);

fn settings(world: &World) -> &Settings {
    world
        .get_resource::<Settings>()
        .unwrap_or_else(|| &DEFAULTS)
}

#[derive(Default)]
struct Help {
    source: Option<HelpSource>,
    text: String,
}

struct HelpSource {
    prefix: Option<&'static str>,
    caption: &'static str,
    chord: String,
}

impl Help {
    fn prepare(
        &mut self,
        prefix: Option<&'static str>,
        caption: &'static str,
        chord: &str,
    ) -> &str {
        if self.source.as_ref().is_none_or(|source| {
            source.prefix != prefix || source.caption != caption || source.chord != chord
        }) {
            let source = self.source.get_or_insert_with(|| HelpSource {
                prefix,
                caption,
                chord: String::new(),
            });
            source.prefix = prefix;
            source.caption = caption;
            source.chord.clear();
            source.chord.push_str(chord);
            self.text.clear();
            if let Some(prefix) = prefix {
                self.text.push_str(prefix);
                self.text.push_str(" · ");
            }
            self.text.push_str(caption);
            self.text.push_str(" · ");
            self.text.push_str(chord);
            engine::performance::add("desktop-control-help-preparations", 1);
        }
        &self.text
    }
}

fn help(world: &mut World, entity: Entity, text: &str) {
    if world
        .get::<crate::native::icons::Tooltip>(entity)
        .is_none_or(|tooltip| tooltip.0 != text)
    {
        world
            .entity_mut(entity)
            .insert(crate::native::icons::Tooltip(text.to_owned()));
        engine::performance::add("desktop-control-tooltip-writes", 1);
    }
    input_settings::accessibility_label(world, entity, text);
}

#[derive(Resource)]
struct Controls {
    roots: QueryState<
        Entity,
        (
            With<Workspaces>,
            With<crate::native::edit_mode::EditMode>,
            With<CanvasView>,
        ),
    >,
    overview: QueryState<(Entity, &'static OverviewControl)>,
    focus: QueryState<(Entity, &'static FocusControl)>,
    tiling: QueryState<(Entity, &'static TilingControl)>,
    root_buffer: Vec<Entity>,
    overview_buffer: Vec<Entity>,
    focus_buffer: Vec<(Entity, Entity, Entity)>,
    tiling_buffer: Vec<(Entity, Entity, Entity)>,
    focus_owners: HashSet<Entity>,
    tiling_owners: HashSet<Entity>,
    overview_help: Help,
    focus_help: [Help; 2],
    tiling_help: [Help; 2],
}

impl FromWorld for Controls {
    fn from_world(world: &mut World) -> Self {
        Self {
            roots: world.query_filtered(),
            overview: world.query(),
            focus: world.query(),
            tiling: world.query(),
            root_buffer: Vec::new(),
            overview_buffer: Vec::new(),
            focus_buffer: Vec::new(),
            tiling_buffer: Vec::new(),
            focus_owners: HashSet::new(),
            tiling_owners: HashSet::new(),
            overview_help: default(),
            focus_help: default(),
            tiling_help: default(),
        }
    }
}

impl Controls {
    fn create(&mut self, world: &mut World) {
        self.root_buffer.clear();
        self.root_buffer.extend(self.roots.iter(world));
        self.focus_owners.clear();
        self.tiling_owners.clear();
        if self.root_buffer.is_empty() {
            return;
        }
        self.focus_owners
            .extend(self.focus.iter(world).map(|(_, control)| control.root));
        self.tiling_owners.extend(
            self.tiling
                .iter(world)
                .filter(|(_, control)| control.toolbar)
                .map(|(_, control)| control.root),
        );
        engine::performance::add(
            "desktop-control-owners-prepared",
            (self.focus_owners.len() + self.tiling_owners.len()) as u64,
        );
        for &root in &self.root_buffer {
            if !self.focus_owners.contains(&root) {
                let toolbar = crate::native::canvas_controls::toolbar(world, root);
                focus_button(world, root, toolbar);
                self.focus_owners.insert(root);
                engine::performance::add("desktop-controls-created", 1);
            }
            if !self.tiling_owners.contains(&root) {
                let toolbar = crate::native::canvas_controls::toolbar(world, root);
                button(world, root, toolbar, true);
                self.tiling_owners.insert(root);
                engine::performance::add("desktop-controls-created", 1);
            }
        }
    }

    fn labels(&mut self, world: &mut World) {
        self.overview_buffer.clear();
        self.overview_buffer.extend(
            self.overview
                .iter(world)
                .filter(|(_, control)| world.get_entity(control.root).is_ok())
                .map(|(entity, _)| entity),
        );
        if self.overview_buffer.is_empty() {
            self.overview_help = default();
        } else {
            let text = self.overview_help.prepare(
                None,
                Token::ShortcutOverview.text(),
                settings(world).text(Shortcut::Overview),
            );
            for &entity in &self.overview_buffer {
                help(world, entity, text);
            }
        }

        self.focus_buffer.clear();
        self.focus_buffer.extend(
            self.focus
                .iter(world)
                .map(|(entity, control)| (entity, control.root, control.label)),
        );
        if self.focus_buffer.is_empty() {
            self.focus_help = default();
        }
        for &(entity, root, label) in &self.focus_buffer {
            focus::remember_selection(world, root);
            let (focused, available) = focus::status(world, root);
            let value = if focused {
                Token::UnfocusTarget.text()
            } else {
                Token::FocusTarget.text()
            };
            input_settings::text(world, label, value);
            input_settings::disable(world, entity, !available);
            let text = self.focus_help[usize::from(focused)].prepare(
                None,
                value,
                settings(world).text(Shortcut::Unfocus),
            );
            help(world, entity, text);
        }

        self.tiling_buffer.clear();
        self.tiling_buffer.extend(
            self.tiling
                .iter(world)
                .map(|(entity, control)| (entity, control.root, control.label)),
        );
        if self.tiling_buffer.is_empty() {
            self.tiling_help = default();
        }
        for &(entity, root, label) in &self.tiling_buffer {
            let tiled = tiled(world, root);
            let value = mode_caption(tiled);
            input_settings::text(world, label, value);
            let text = self.tiling_help[usize::from(tiled)].prepare(
                Some(Token::ShortcutTiling.text()),
                value,
                settings(world).text(Shortcut::Tiling),
            );
            help(world, entity, text);
        }
        engine::performance::add(
            "desktop-control-label-consumers",
            (self.overview_buffer.len() + self.focus_buffer.len() + self.tiling_buffer.len())
                as u64,
        );
    }
}

pub(super) fn create(world: &mut World) {
    let _maintenance = engine::performance::Span::new("desktop-control-maintenance");
    world.init_resource::<Controls>();
    world.resource_scope(|world, mut controls: Mut<Controls>| controls.create(world));
}

pub(super) fn labels(world: &mut World) {
    let _presentation = engine::performance::Span::new("desktop-control-presentation");
    world.init_resource::<Controls>();
    world.resource_scope(|world, mut controls: Mut<Controls>| controls.labels(world));
}
