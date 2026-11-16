use super::*;
use bevy::ecs::query::QueryState;
use std::collections::HashMap;

mod rows;

#[derive(Default)]
struct Feedback {
    source: [Option<String>; 4],
    value: String,
}

impl Feedback {
    fn prepare(&mut self, stale: bool, problem: Option<OrderProblem<'_>>, message: &str) -> &str {
        let prefix = if stale {
            [Some("stale"), None, None]
        } else {
            match &problem {
                Some(OrderProblem::Invalid { uid, error }) => {
                    [Some("invalid"), Some(*uid), Some(*error)]
                }
                Some(OrderProblem::Loading) => [Some("loading"), None, None],
                None => [None, None, None],
            }
        };
        let source = [prefix[0], prefix[1], prefix[2], Some(message)];
        if self.source.each_ref().map(|value| value.as_deref()) != source {
            self.source = source.map(|value| value.map(str::to_owned));
            self.value.clear();
            if stale {
                self.value.push_str("Saved rule order changed. Your draft is kept; Reload active rule order or Review it against the file.");
            } else if let Some(problem) = &problem {
                use std::fmt::Write;
                let _ = write!(self.value, "{problem}");
            }
            if (stale || problem.is_some()) && !message.is_empty() {
                self.value.push('\n');
            }
            self.value.push_str(message);
            engine::performance::add("settings-feedback-prepared", 1);
        }
        &self.value
    }
}

enum WindowSource {
    Live {
        properties: Properties,
        matched: Option<Match>,
        warning: Option<String>,
    },
    Unavailable(String),
}

#[derive(Default)]
struct WindowView {
    source: Option<WindowSource>,
    description: String,
}

impl WindowView {
    fn prepare(&mut self, decision: Option<&Decision>, last: &str) -> &str {
        let unchanged = match (&self.source, decision) {
            (
                Some(WindowSource::Live {
                    properties,
                    matched,
                    warning,
                }),
                Some(decision),
            ) => {
                properties == &decision.properties
                    && matched == &decision.matched
                    && warning == &decision.warning
            }
            (Some(WindowSource::Unavailable(previous)), None) => previous == last,
            _ => false,
        };
        if !unchanged {
            self.description = if let Some(decision) = decision {
                let matched = decision
                    .matched
                    .as_ref()
                    .map(|matched| {
                        format!(
                            "Rule: {} · {} · revision {}",
                            matched.title, matched.uid, matched.revision
                        )
                    })
                    .unwrap_or_else(|| {
                        "No matching application rule; normal or retained preferences were used."
                            .into()
                    });
                format!(
                    "{matched}\nRequested: {}{}",
                    fields::summary(&decision.properties),
                    decision
                        .warning
                        .as_ref()
                        .map(|warning| format!("\n{warning}"))
                        .unwrap_or_default()
                )
            } else if last.is_empty() {
                "The live opening decision is unavailable.".into()
            } else {
                format!(
                    "{}\nApplication closed or unavailable.",
                    last.lines()
                        .filter(|line| *line != "Application closed or unavailable.")
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            };
            self.source = Some(match decision {
                Some(decision) => WindowSource::Live {
                    properties: decision.properties.clone(),
                    matched: decision.matched.clone(),
                    warning: decision.warning.clone(),
                },
                None => WindowSource::Unavailable(last.to_owned()),
            });
            engine::performance::add("opening-window-descriptions-prepared", 1);
        }
        &self.description
    }
}

#[derive(Resource)]
struct Presentation {
    pages: QueryState<Entity, With<Page>>,
    windows: QueryState<Entity, With<WindowControls>>,
    pending_pages: Vec<Entity>,
    pending_windows: Vec<Entity>,
    rows: HashMap<Entity, rows::Rows>,
    feedback: HashMap<Entity, Feedback>,
    descriptions: HashMap<Entity, WindowView>,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            pages: world.query_filtered(),
            windows: world.query_filtered(),
            pending_pages: Vec::new(),
            pending_windows: Vec::new(),
            rows: HashMap::new(),
            feedback: HashMap::new(),
            descriptions: HashMap::new(),
        }
    }
}

pub(super) fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("opening-settings-presentation");
    let mut presentation = world
        .remove_resource::<Presentation>()
        .unwrap_or_else(|| Presentation::from_world(world));
    render_pages(world, &mut presentation);
    render_windows(world, &mut presentation);
    world.insert_resource(presentation);
}

fn render_pages(world: &mut World, presentation: &mut Presentation) {
    let _presentation = engine::performance::Span::new("opening-rule-page-presentation");
    presentation.pending_pages.clear();
    presentation
        .pending_pages
        .extend(presentation.pages.iter(world));
    presentation
        .rows
        .retain(|page, _| world.get::<Page>(*page).is_some());
    presentation
        .feedback
        .retain(|page, _| world.get::<Page>(*page).is_some());
    for page in presentation.pending_pages.drain(..) {
        let state = world.get::<Page>(page).unwrap();
        let active = active_rules(world);
        if state.order == state.origin && state.origin != active {
            let active = active.to_vec();
            let mut state = world.get_mut::<Page>(page).unwrap();
            state.order.clone_from(&active);
            state.origin = active;
        }
        let state = world.get::<Page>(page).unwrap();
        let (body, toggle, list, status, save, expanded) = (
            state.body,
            state.toggle,
            state.list,
            state.status,
            state.save,
            state.expanded,
        );
        let stale = state.origin != active_rules(world);
        // These checks remain fresh even when their text is unchanged.
        let problem = if stale {
            None
        } else {
            order_problem(world, &state.order)
        };
        let blocked = state.order == active_rules(world) || stale || problem.is_some();
        let message =
            presentation
                .feedback
                .entry(page)
                .or_default()
                .prepare(stale, problem, &state.message);
        display(
            world,
            body,
            if expanded {
                Display::Flex
            } else {
                Display::None
            },
        );
        caption(
            world,
            toggle,
            if expanded {
                "Hide opening rules"
            } else {
                "Edit opening rules"
            },
        );
        presentation
            .rows
            .entry(page)
            .or_default()
            .refresh(world, page, list, toggle.0);
        text(world, status, message);
        disable(world, save, blocked);
    }
}

fn render_windows(world: &mut World, presentation: &mut Presentation) {
    let _presentation = engine::performance::Span::new("opening-window-presentation");
    presentation.pending_windows.clear();
    presentation
        .pending_windows
        .extend(presentation.windows.iter(world));
    presentation
        .descriptions
        .retain(|panel, _| world.get::<WindowControls>(*panel).is_some());
    for panel in presentation.pending_windows.drain(..) {
        let controls = world.get::<WindowControls>(panel).unwrap();
        let target = captured_target(world, controls);
        let decision = target
            .as_ref()
            .and_then(|(entity, _)| world.get::<Decision>(*entity));
        let (info, reapply) = (controls.info, controls.reapply);
        let matched = decision
            .filter(|decision| controls.matched != decision.matched)
            .map(|decision| decision.matched.clone());
        let description = presentation
            .descriptions
            .entry(panel)
            .or_default()
            .prepare(decision, &controls.last);
        let last_changed = controls.last != description;
        let unavailable = target.is_none() || !world.contains_resource::<HostRequests>();
        if matched.is_some() || last_changed {
            let mut controls = world.get_mut::<WindowControls>(panel).unwrap();
            if let Some(matched) = matched {
                controls.matched = matched;
            }
            if last_changed {
                description.clone_into(&mut controls.last);
            }
        }
        text(world, info, description);
        disable(world, reapply, unavailable);
    }
}
