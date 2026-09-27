use crate::tokens::Token;
use bevy::prelude::*;

#[derive(Component, Clone, Copy)]
pub struct BackgroundToken(pub Token);
#[derive(Component, Clone, Copy)]
pub struct TextToken(pub Token);
#[derive(Component, Clone, Copy)]
pub struct BorderToken(pub Token);
#[derive(Component, Clone, Copy)]
pub struct OutlineToken(pub Token);
#[derive(Component, Clone, Copy)]
pub struct CursorToken(pub Token);
pub fn background(token: Token) -> impl Bundle {
    (
        BackgroundColor(token.default_value(Default::default()).color()),
        BackgroundToken(token),
    )
}

pub fn text(token: Token) -> impl Bundle {
    (
        TextColor(token.default_value(Default::default()).color()),
        TextToken(token),
    )
}

pub fn border(token: Token) -> impl Bundle {
    (
        BorderColor::all(token.default_value(Default::default()).color()),
        BorderToken(token),
    )
}
