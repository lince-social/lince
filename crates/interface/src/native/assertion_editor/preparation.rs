// non-lean-yet
// Scope: struct Preparation.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 52.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (134).

use super::*;
use crate::host::performance::{self, Count, Stage};
use std::collections::HashSet;

pub(super) struct Preparation {
    cursor: bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>,
    field_query: QueryState<Entity, With<Field>>,
    chip_query: QueryState<(Entity, &'static Chip)>,
    pub(super) messages: Vec<Arc<ServerMessage>>,
    pub(super) stale: Vec<Entity>,
    pub(super) fields: Vec<Entity>,
    chips: Vec<(Entity, Entity)>,
    hovered: HashSet<Entity>,
}

impl FromWorld for Preparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            cursor: Default::default(),
            field_query: world.query_filtered(),
            chip_query: world.query(),
            messages: Vec::new(),
            stale: Vec::new(),
            fields: Vec::new(),
            chips: Vec::new(),
            hovered: HashSet::new(),
        }
    }
}

impl Preparation {
    pub(super) fn messages(&mut self, world: &World) {
        let _prepare = performance::Span::new(Stage::AssertionEditorPrepare);
        self.messages.clear();
        if let Some(messages) =
            world.get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        {
            if world.resource::<Subscriptions>().0.is_empty() {
                self.cursor.clear(messages);
            } else {
                self.messages.extend(
                    self.cursor
                        .read(messages)
                        .filter(|message| {
                            matches!(
                                message.0.as_ref(),
                                ServerMessage::Snapshot { .. }
                                    | ServerMessage::Update { .. }
                                    | ServerMessage::Error { .. }
                            )
                        })
                        .map(|message| message.0.clone()),
                );
            }
        }
        performance::add(Count::AssertionMessagesPrepared, self.messages.len() as u64);
        if self.messages.is_empty() {
            self.messages.shrink_to_fit();
        }
    }

    pub(super) fn stale(&mut self, world: &World) {
        let _prepare = performance::Span::new(Stage::AssertionEditorPrepare);
        self.stale.clear();
        self.stale.extend(
            world
                .resource::<Subscriptions>()
                .0
                .iter()
                .filter(|(entity, (_, sender))| {
                    world.get::<Field>(**entity).is_none() || sender.is_closed()
                })
                .map(|(entity, _)| *entity),
        );
        if self.stale.is_empty() {
            self.stale.shrink_to_fit();
        }
    }

    pub(super) fn fields(&mut self, world: &World) {
        let _prepare = performance::Span::new(Stage::AssertionEditorPrepare);
        self.fields.clear();
        self.fields.extend(self.field_query.iter(world));
        performance::add(Count::AssertionFieldsPrepared, self.fields.len() as u64);
        if self.fields.is_empty() {
            self.fields.shrink_to_fit();
        }
    }

    pub(super) fn chips(&mut self, world: &mut World) {
        let _chips = performance::Span::new(Stage::AssertionChipPresentation);
        let prepare = performance::Span::new(Stage::AssertionEditorPrepare);
        self.chips.clear();
        self.chips.extend(
            self.chip_query
                .iter(world)
                .map(|(entity, chip)| (entity, chip.close)),
        );
        performance::add(Count::AssertionChipsPrepared, self.chips.len() as u64);
        self.hovered.clear();
        if self.chips.is_empty() {
            self.chips.shrink_to_fit();
            self.hovered.shrink_to_fit();
            return;
        }
        crate::native::pointer_hit::extend_ancestors(
            world,
            &mut self.hovered,
            world
                .get_resource::<bevy::picking::hover::HoverMap>()
                .into_iter()
                .flat_map(|hover| hover.values().flat_map(|hits| hits.keys().copied())),
        );
        performance::add(Count::AssertionHoverAncestors, self.hovered.len() as u64);
        if self.hovered.is_empty() {
            self.hovered.shrink_to_fit();
        }
        let focus = world.resource::<InputFocus>().get();
        drop(prepare);
        for (entity, close) in self.chips.drain(..) {
            let active = focus == Some(close) || self.hovered.contains(&entity);
            let color = if active {
                crate::native::token_style::resolve(world, entity, Token::Accent)
                    .0
                    .color()
            } else {
                Color::NONE
            };
            if world.get::<BorderColor>(entity) != Some(&BorderColor::all(color)) {
                world.entity_mut(entity).insert(BorderColor::all(color));
            }
            world
                .get_mut::<Visibility>(close)
                .unwrap()
                .set_if_neq(if active {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
        }
        self.hovered.clear();
    }
}
