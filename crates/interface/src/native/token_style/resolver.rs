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
}

impl Tokens<'_, '_> {
    pub(crate) fn metric(&self, entity: Entity, usage: &str, fallback: Token) -> TokenValue {
        let token = self
            .inputs
            .get(entity)
            .ok()
            .and_then(|input| input.uses)
            .and_then(|uses| uses.0.get(usage))
            .copied()
            .unwrap_or(fallback);
        self.resolve(entity, token).0
    }

    pub(crate) fn resolve(&self, entity: Entity, token: Token) -> (TokenValue, &'static str) {
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
                return self.resolve(source.0, token);
            }
            ancestor = input.parent.map(ChildOf::parent);
        }
        self.theme
            .resolve(token, style_kind, &TokenOverrides::default())
    }
}
