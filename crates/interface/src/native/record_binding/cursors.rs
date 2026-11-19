use super::*;
use bevy::platform::collections::Equivalent;
use std::hash::{Hash, Hasher};

mod state;
pub(super) use state::Cursors;
use state::Snapshot;

type Map<K, V> =
    bevy::platform::collections::HashMap<K, V, std::collections::hash_map::RandomState>;

#[derive(Eq, PartialEq)]
struct Key {
    editor: Entity,
    session: String,
}

struct KeyRef<'a> {
    editor: Entity,
    session: &'a str,
}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.editor, self.session.as_str()).hash(state);
    }
}

impl Hash for KeyRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.editor, self.session).hash(state);
    }
}

impl Equivalent<Arc<Key>> for KeyRef<'_> {
    fn equivalent(&self, key: &Arc<Key>) -> bool {
        self.editor == key.editor && self.session == key.session.as_str()
    }
}

#[derive(Component)]
struct RemoteCursor(Arc<Key>);

struct Paint {
    editor: Entity,
    byte: usize,
    snapshot: Arc<Snapshot>,
    index: usize,
}

pub(super) struct Presentation {
    carets: QueryState<(Entity, &'static RemoteCursor)>,
    editors: QueryState<(Entity, &'static TextBinding, &'static EditableText)>,
    old: Map<Arc<Key>, Entity>,
    pending: Vec<Paint>,
    positions: Vec<(usize, usize)>,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            carets: world.query(),
            editors: world.query(),
            old: Map::default(),
            pending: Vec::new(),
            positions: Vec::new(),
        }
    }
}

impl Presentation {
    fn clear(&mut self) {
        self.old.clear();
        self.old.shrink_to_fit();
        self.pending.clear();
        self.pending.shrink_to_fit();
        self.positions.clear();
        self.positions.shrink_to_fit();
    }

    fn prepare(&mut self, world: &World, bindings: &mut Bindings) -> bool {
        let _prepare = performance::Span::new(Stage::RemoteCursorPrepare);
        let mut present = false;
        for document in bindings.documents.values_mut() {
            present |= document.cursors.begin_paint();
        }
        if !present {
            return false;
        }
        for (entity, binding, text) in self.editors.iter(world) {
            if crate::native::laboratory::blocked(world, entity) {
                continue;
            }
            let Some(document) = bindings.documents.get_mut(&key_ref(&binding.record)) else {
                continue;
            };
            let Some(snapshot) = document.cursors.snapshot() else {
                continue;
            };
            self.positions.clear();
            for &index in snapshot.indices(&binding.property) {
                if let Some(position) = document.cursors.position(&document.doc, index) {
                    self.positions.push((position, self.pending.len()));
                    self.pending.push(Paint {
                        editor: entity,
                        byte: 0,
                        snapshot: snapshot.clone(),
                        index,
                    });
                }
            }
            let _bytes = performance::Span::new(Stage::RemoteCursorBytes);
            self.positions.sort_unstable();
            let value = text.value();
            let mut chars = value.chars();
            let (mut scalar, mut byte, mut ended) = (0, 0, false);
            for &(position, pending) in &self.positions {
                while scalar < position && !ended {
                    if let Some(character) = chars.next() {
                        scalar += 1;
                        byte += character.len_utf8();
                    } else {
                        ended = true;
                    }
                }
                self.pending[pending].byte = byte;
            }
            performance::add(Count::RemoteCursorCharacters, scalar as u64);
        }
        true
    }

    pub(super) fn paint(&mut self, world: &mut World) {
        let _paint = performance::Span::new(Stage::RemoteCursorPaint);
        self.old.clear();
        self.old.extend(
            self.carets
                .iter(world)
                .map(|(entity, cursor)| (cursor.0.clone(), entity)),
        );
        let Some(mut bindings) = world.remove_resource::<Bindings>() else {
            self.clear();
            return;
        };
        let present = self.prepare(world, &mut bindings);
        performance::add(Count::RemoteCursorCandidates, self.pending.len() as u64);
        for Paint {
            editor: entity,
            byte,
            snapshot,
            index,
        } in self.pending.drain(..)
        {
            let wire = &snapshot.wire[index];
            let text = world.get::<EditableText>(entity).unwrap();
            let geometry = if let Some(layout) = text.editor.try_layout() {
                let affinity = if byte == text.value().into_iter().map(str::len).sum::<usize>() {
                    parley::Affinity::Upstream
                } else {
                    parley::Affinity::Downstream
                };
                text.editor.cursor_geometry(2.0).map(|_| {
                    parley::Cursor::from_byte_index(layout, byte, affinity).geometry(layout, 2.0)
                })
            } else {
                let Some(mut fonts) = world.remove_resource::<FontCx>() else {
                    break;
                };
                let Some(mut layout) = world.remove_resource::<LayoutCx>() else {
                    world.insert_resource(fonts);
                    break;
                };
                let mut editor = world.get::<EditableText>(entity).unwrap().editor.clone();
                editor
                    .driver(&mut fonts.context, &mut layout.0)
                    .move_to_byte(byte);
                let geometry = editor.cursor_geometry(2.0);
                world.insert_resource(fonts);
                world.insert_resource(layout);
                geometry
            };
            if let Some(rect) = geometry {
                let computed = world
                    .get::<ComputedNode>(entity)
                    .copied()
                    .unwrap_or_default();
                let scroll = world
                    .get::<bevy::ui::widget::TextScroll>(entity)
                    .map_or(Vec2::ZERO, |scroll| scroll.0);
                let origin = Vec2::new(rect.x0 as f32, rect.y0 as f32) - scroll;
                let bottom = (rect.y1 as f32 - scroll.y).min(computed.content_box().height());
                if computed.size != Vec2::ZERO
                    && (origin.x < 0.0
                        || origin.x > computed.content_box().width()
                        || bottom <= 0.0
                        || origin.y >= computed.content_box().height())
                {
                    continue;
                }
                let scale = computed.inverse_scale_factor;
                let node = Node {
                    position_type: PositionType::Absolute,
                    left: px((computed.padding.min_inset.x + origin.x) * scale),
                    top: px((computed.padding.min_inset.y + origin.y.max(0.0)) * scale),
                    width: px(2),
                    height: px((bottom - origin.y.max(0.0)).max(0.0) * scale),
                    ..default()
                };
                if let Some(cursor) = self.old.remove(&KeyRef {
                    editor: entity,
                    session: &wire.session,
                }) {
                    if world.get::<Node>(cursor) != Some(&node) {
                        world.entity_mut(cursor).insert(node);
                        performance::count(Count::RemoteCursorGeometryWrites);
                    }
                } else {
                    let caret = world
                        .spawn((
                            RemoteCursor(Arc::new(Key {
                                editor: entity,
                                session: wire.session.clone(),
                            })),
                            node,
                            Pickable::IGNORE,
                            crate::native::token_style::background(
                                crate::native::tokens::Token::Accent,
                            ),
                            ChildOf(entity),
                        ))
                        .id();
                    performance::count(Count::RemoteCursorCreated);
                    if world.contains_resource::<crate::native::theme::Typography>() {
                        let name = wire
                            .person
                            .as_deref()
                            .or(wire.organ.as_deref())
                            .unwrap_or("Local editor");
                        let label = crate::native::edit_mode::label(world, caret, name, 10.0);
                        world.entity_mut(label).insert((
                            BindingStatus,
                            Pickable::IGNORE,
                            Node {
                                position_type: PositionType::Absolute,
                                bottom: percent(100),
                                left: px(0),
                                width: px(180),
                                ..default()
                            },
                            TextLayout::linebreak(bevy::text::LineBreak::NoWrap),
                        ));
                    }
                }
            }
        }
        for (_, entity) in self.old.drain() {
            world.despawn(entity);
            performance::count(Count::RemoteCursorRetired);
        }
        if !present {
            self.clear();
        }
        world.insert_resource(bindings);
    }
}
