use crate::canvas::CanvasView;
use bevy::{input_focus::tab_navigation::TabGroup, prelude::*};

#[derive(Component, Default)]
#[require(Node = viewport(), TabGroup, CanvasView)]
pub struct BoxRoot;

fn viewport() -> Node {
    Node {
        width: percent(100),
        height: percent(100),
        overflow: Overflow::clip(),
        ..default()
    }
}
