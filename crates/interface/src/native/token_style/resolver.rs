use super::*;
use crate::native::{
    canvas_host::authoring::AuthoredTokenUses,
    relation_castle::styles::{Color as RuleColor, Colors},
    sand_store::{PreviewKind, PreviewSource},
};
use bevy::ecs::{
    entity_disabling::Disabled,
    query::{Allow, QueryData},
    system::SystemParam,
};
use std::collections::HashMap;

pub(crate) mod tests;

#[derive(QueryData)]
struct Inputs {
    parent: Option<&'static ChildOf>,
    overrides: Option<&'static TokenOverrides>,
    text: Option<&'static SandText>,
    preview: Option<&'static PreviewKind>,
    source: Option<&'static PreviewSource>,
    stored: Option<&'static StoredSand>,
    record: Has<RecordPlacement>,
    item: Has<CanvasItem>,
    binding: Has<crate::native::protein_area::RecordBinding>,
    castle: Has<crate::native::castle::Castle>,
    fiote: Has<crate::native::fiote::Fiote>,
    map: Has<crate::native::map_castle::MapSand>,
    time: Has<crate::native::time_castle::TimeSettings>,
    colors: Option<&'static Colors>,
    uses: Option<&'static AuthoredTokenUses>,
}

impl InputsItem<'_, '_> {
    fn kind(&self) -> Option<SandStyleKind> {
        if self.fiote {
            return Some(SandStyleKind::Fiote);
        }
        if self.map {
            return Some(SandStyleKind::Square);
        }
        if self.time {
            return Some(SandStyleKind::TimeCastle);
        }
        if let Some(preview) = self.preview {
            return Some(preview.0);
        }
        if let Some(text) = self.text {
            return Some(if text.editable {
                SandStyleKind::EditableText
            } else {
                SandStyleKind::Text
            });
        }
        if let Some(stored) = self.stored {
            return Some(stored_kind(stored.kind));
        }
        if self.record {
            return Some(SandStyleKind::Record);
        }
        (self.item && self.binding).then_some(if self.castle {
            SandStyleKind::Record
        } else {
            SandStyleKind::Square
        })
    }
}

#[derive(SystemParam)]
pub(crate) struct Tokens<'w, 's> {
    inputs: Query<'w, 's, Inputs, Allow<Disabled>>,
    theme: Res<'w, ThemeSettings>,
    changed: Query<
        'w,
        's,
        (),
        (
            Allow<Disabled>,
            Or<(
                Changed<ChildOf>,
                Changed<TokenOverrides>,
                Changed<SandText>,
                Changed<PreviewKind>,
                Changed<PreviewSource>,
                Changed<StoredSand>,
                Changed<Colors>,
                Changed<AuthoredTokenUses>,
                Added<RecordPlacement>,
                Added<CanvasItem>,
                Added<crate::native::protein_area::RecordBinding>,
                Added<crate::native::castle::Castle>,
                Added<crate::native::fiote::Fiote>,
                Added<crate::native::map_castle::MapSand>,
                Added<crate::native::time_castle::TimeSettings>,
            )>,
        ),
    >,
    removed: Removed<'w, 's>,
    cache: Local<'s, Cache>,
}

#[derive(Default)]
struct Cache {
    values: HashMap<(Entity, usize), (TokenValue, &'static str)>,
    kinds: HashMap<Entity, Option<SandStyleKind>>,
}

macro_rules! removals {
    ($($field:ident: $component:ty),* $(,)?) => {
        #[derive(SystemParam)]
        struct Removed<'w, 's> {
            $($field: RemovedComponents<'w, 's, $component>,)*
        }
        impl Removed<'_, '_> {
            fn changed(&mut self) -> bool {
                let mut changed = false;
                $(changed |= self.$field.read().count() != 0;)*
                changed
            }
        }
    };
}

removals! {
    parents: ChildOf, overrides: TokenOverrides, text: SandText, previews: PreviewKind,
    sources: PreviewSource, stored: StoredSand, colors: Colors, uses: AuthoredTokenUses,
    records: RecordPlacement, items: CanvasItem, bindings: crate::native::protein_area::RecordBinding,
    castles: crate::native::castle::Castle, fiotes: crate::native::fiote::Fiote,
    maps: crate::native::map_castle::MapSand, clocks: crate::native::time_castle::TimeSettings,
    nodes: Node, icons: crate::native::icons::IconStyle, backgrounds: BackgroundToken,
    labels: TextToken, borders: BorderToken, outlines: OutlineToken, cursors: CursorToken,
}

impl Tokens<'_, '_> {
    pub(crate) fn refresh(&mut self) {
        let removed = self.removed.changed();
        if removed || self.theme.is_changed() || !self.changed.is_empty() {
            self.cache.values.clear();
            self.cache.kinds.clear();
        }
    }

    pub(crate) fn prepare(&mut self, entity: Entity, tokens: impl IntoIterator<Item = Token>) {
        for token in tokens {
            let key = (entity, token as usize);
            if !self.cache.values.contains_key(&key) {
                let value = self.resolve_direct(entity, token);
                self.cache.values.insert(key, value);
            }
        }
    }

    pub(crate) fn prepare_metric(&mut self, entity: Entity, usage: &str, fallback: Token) {
        self.prepare(entity, [self.metric_token(entity, usage, fallback)]);
    }

    pub(crate) fn prepare_kind(&mut self, entity: Entity) {
        if !self.cache.kinds.contains_key(&entity) {
            let kind = self.inherited_kind(entity);
            self.cache.kinds.insert(entity, kind);
        }
    }

    pub(crate) fn finish_preparing(&mut self) {
        if self.cache.values.capacity() > (self.cache.values.len() * 4).max(256) {
            self.cache.values.shrink_to_fit();
        }
        if self.cache.kinds.capacity() > (self.cache.kinds.len() * 4).max(64) {
            self.cache.kinds.shrink_to_fit();
        }
    }

    pub(crate) fn kind(&self, entity: Entity) -> Option<SandStyleKind> {
        self.inputs.get(entity).ok().and_then(|input| input.kind())
    }

    pub(crate) fn inherited_kind(&self, entity: Entity) -> Option<SandStyleKind> {
        if let Some(kind) = self.cache.kinds.get(&entity) {
            return *kind;
        }
        let mut ancestor = Some(entity);
        while let Some(entity) = ancestor {
            let input = self.inputs.get(entity).ok()?;
            if let Some(kind) = input.kind() {
                return Some(kind);
            }
            ancestor = input.parent.map(ChildOf::parent);
        }
        None
    }

    pub(crate) fn has_use(&self, entity: Entity, usage: &str) -> bool {
        self.inputs
            .get(entity)
            .ok()
            .and_then(|input| input.uses)
            .is_some_and(|uses| uses.0.contains_key(usage))
    }

    pub(crate) fn clock(&self, entity: Entity) -> bool {
        self.inputs.get(entity).is_ok_and(|input| input.time)
    }

    pub(crate) fn metric(&self, entity: Entity, usage: &str, fallback: Token) -> TokenValue {
        self.resolve(entity, self.metric_token(entity, usage, fallback))
            .0
    }

    fn metric_token(&self, entity: Entity, usage: &str, fallback: Token) -> Token {
        self.inputs
            .get(entity)
            .ok()
            .and_then(|input| input.uses)
            .and_then(|uses| uses.0.get(usage))
            .copied()
            .unwrap_or(fallback)
    }

    pub(crate) fn resolve(&self, entity: Entity, token: Token) -> (TokenValue, &'static str) {
        self.cache
            .values
            .get(&(entity, token as usize))
            .copied()
            .unwrap_or_else(|| self.resolve_direct(entity, token))
    }

    fn resolve_direct(&self, entity: Entity, token: Token) -> (TokenValue, &'static str) {
        if matches!(
            token,
            Token::SandBackground
                | Token::Surface
                | Token::SandInk
                | Token::Ink
                | Token::SandBorder
                | Token::Accent
                | Token::BorderWidth
        ) {
            let mut ancestor = Some(entity);
            while let Some(current) = ancestor {
                let Ok(input) = self.inputs.get(current) else {
                    break;
                };
                if let Some(colors) = input.colors {
                    let color = match token {
                        Token::SandBackground | Token::Surface => colors.background.as_ref(),
                        Token::SandInk | Token::Ink => {
                            colors.text.as_ref().or(colors.link.as_ref())
                        }
                        Token::SandBorder => colors.border.as_ref(),
                        Token::Accent => colors.link.as_ref(),
                        Token::BorderWidth if colors.border.is_some() && current == entity => {
                            return (
                                TokenValue::Number(self.base(current, token).0.number().max(1.0)),
                                "Relation color rule",
                            );
                        }
                        _ => None,
                    };
                    if let Some(color) = color {
                        let value = match color {
                            RuleColor::Specific(rgba) => TokenValue::Color(*rgba),
                            RuleColor::Token(reference) => self.base(current, *reference).0,
                        };
                        return (value, "Relation color rule");
                    }
                }
                ancestor = input.parent.map(ChildOf::parent);
            }
        }
        self.base(entity, token)
    }

    fn base(&self, entity: Entity, token: Token) -> (TokenValue, &'static str) {
        let mut ancestor = Some(entity);
        let mut style_kind = None;
        while let Some(current) = ancestor {
            let Ok(input) = self.inputs.get(current) else {
                break;
            };
            style_kind = style_kind.or_else(|| input.kind());
            let values = input
                .overrides
                .or_else(|| input.text.map(|text| &text.tokens));
            if let Some(value) = values.and_then(|values| values.0.get(&token)) {
                return (
                    *value,
                    if current == entity {
                        "This Sand"
                    } else {
                        "Parent Sand"
                    },
                );
            }
            if let Some(source) = input.source {
                return self.resolve_direct(source.0, token);
            }
            ancestor = input.parent.map(ChildOf::parent);
        }
        self.theme
            .resolve(token, style_kind, &TokenOverrides::default())
    }
}
