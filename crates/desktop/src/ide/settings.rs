use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub autosave_seconds: u16,
    pub search_visible: bool,
    pub search: lince_editor::search::Options,
    pub sidebar_width: f32,
    pub project_search: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autosave_seconds: 0,
            search_visible: false,
            search: Default::default(),
            sidebar_width: 270.0,
            project_search: false,
        }
    }
}

impl Settings {
    pub(super) fn valid(&self) -> bool {
        matches!(self.autosave_seconds, 0 | 2 | 5)
            && self.sidebar_width.is_finite()
            && (180.0..=600.0).contains(&self.sidebar_width)
    }
}

pub(super) fn panel(world: &mut World, owner: Entity) -> (Entity, [Entity; 4]) {
    let panel = crate::sand_panel::row(world, owner);
    world.get_mut::<Node>(panel).unwrap().display = Display::None;
    world.get_mut::<Node>(panel).unwrap().flex_wrap = FlexWrap::Wrap;
    let labels = [
        ("Autosave: off", actions::Control::Autosave),
        ("Match case: on", actions::Control::MatchCase),
        ("Whole word: off", actions::Control::WholeWord),
        (
            "Project search: off",
            actions::Control::ProjectSearchSetting,
        ),
    ]
    .map(|(caption, action)| {
        let button = crate::sand_panel::button(world, panel, owner, caption, action);
        world.get::<Children>(button).unwrap()[0]
    });
    (panel, labels)
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let settings = world.get::<Ide>(owner).unwrap().settings;
    let view = world.get::<View>(owner).unwrap();
    if view.shown_settings == Some(settings) {
        return;
    }
    let (navigation, explorer, labels) = (view.navigation, view.explorer, view.settings_labels);
    world.get_mut::<Node>(navigation).unwrap().display = if settings.search_visible {
        Display::Flex
    } else {
        Display::None
    };
    world.get_mut::<Node>(explorer).unwrap().width = px(settings.sidebar_width);
    let captions = [
        if settings.autosave_seconds == 0 {
            "Autosave: off".into()
        } else {
            format!("Autosave: {} s", settings.autosave_seconds)
        },
        format!(
            "Match case: {}",
            if settings.search.case_sensitive {
                "on"
            } else {
                "off"
            }
        ),
        format!(
            "Whole word: {}",
            if settings.search.whole_word {
                "on"
            } else {
                "off"
            }
        ),
    ];
    for (entity, caption) in labels.into_iter().zip(captions.into_iter().chain([format!(
        "Project search: {}",
        if settings.project_search { "on" } else { "off" }
    )])) {
        crate::sand_panel::status(world, entity, caption.clone());
        let button = world.get::<ChildOf>(entity).unwrap().parent();
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) {
            node.set_label(caption.clone());
        }
        world.get_mut::<crate::icons::Tooltip>(button).unwrap().0 = caption;
    }
    world.get_mut::<View>(owner).unwrap().shown_settings = Some(settings);
}

pub(super) fn divider(world: &mut World, parent: Entity, owner: Entity) {
    world
        .spawn((
            ChildOf(parent),
            Node {
                width: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Accent),
            crate::icons::Tooltip("Drag to resize Explorer".into()),
        ))
        .observe(
            move |mut drag: On<Pointer<Drag>>,
                  transforms: Query<&UiGlobalTransform>,
                  mut configs: Query<&mut Ide>| {
                drag.propagate(false);
                if drag.button != PointerButton::Primary {
                    return;
                }
                let Ok(mut ide) = configs.get_mut(owner) else {
                    return;
                };
                let delta = transforms
                    .get(drag.entity)
                    .map_or(drag.delta.x, |transform| {
                        transform.inverse().transform_vector2(drag.delta).x
                    });
                if delta.is_finite() {
                    ide.settings.sidebar_width =
                        (ide.settings.sidebar_width + delta).clamp(180.0, 600.0);
                }
            },
        );
}
