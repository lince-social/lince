//! Shared visibility rule editor and host message interface.

mod commands;
mod draft;
mod render;
#[cfg(any(test, feature = "native-presentation"))]
mod tests;

use std::{
    sync::{Arc, OnceLock},
    time::Instant,
};

use bevy::prelude::*;
use nucleus::visibility::{Command, Context, Data, Policy};

use crate::controls;

use commands::{maintain, request};
pub use commands::{receive, receive_ref};
use draft::{BoundEnd, Rule, Side, prepared_policy, read_fields};
use render::draw;

#[derive(Message, Clone)]
pub struct Request {
    pub owner: Entity,
    pub id: String,
    pub command: Command,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Selection {
    pub record_uid: String,
    pub data: Data,
}

impl Selection {
    pub fn valid(&self) -> bool {
        self.record_uid.is_empty() || nucleus::valid_uid(&self.record_uid, "r")
    }
}

pub fn selection(world: &World, entity: Entity) -> Option<Selection> {
    world.get::<Panel>(entity).map(|panel| Selection {
        record_uid: panel.record.clone(),
        data: panel.data,
    })
}

#[derive(Component)]
pub struct Panel {
    draft: Option<Side>,
    owner: Entity,
    record: String,
    data: Data,
    context: Option<Context>,
    include: Vec<Rule>,
    exclude: Vec<Rule>,
    prepared_policy: OnceLock<Result<Arc<Policy>, String>>,
    fields: Vec<(Side, usize, BoundEnd, Entity)>,
    radius_fields: Vec<(usize, Entity)>,
    pending: Option<(String, Instant)>,
    notice: String,
    choosing_record: bool,
    choosing_organs: Option<(Side, usize)>,
    show_credits: bool,
}

#[derive(Clone)]
enum Intent {
    ChooseRecord,
    Record(String),
    Data(Data),
    Add(Side),
    Remove(Side, usize),
    Organ(Side, usize, String),
    Person(Side, usize, String),
    ChooseOrgans(Side, usize),
    Inclusive(Side, usize, BoundEnd),
    Save,
    Refresh,
    Preview,
    Credits,
}

#[derive(Component, Clone)]
struct Button {
    panel: Entity,
    intent: Intent,
}

pub struct VisibilityUiPlugin;

impl Plugin for VisibilityUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Request>().add_systems(Update, maintain);
    }
}

pub fn mount(world: &mut World, parent: Entity, record: &str, data: Data) -> Entity {
    let panel = controls::column(world, parent);
    let mut state = Panel {
        draft: None,
        owner: panel,
        record: record.into(),
        data,
        context: None,
        include: vec![],
        exclude: vec![],
        prepared_policy: OnceLock::new(),
        fields: vec![],
        radius_fields: vec![],
        pending: None,
        notice: "Choose a Record and the data whose visibility you want to control".into(),
        choosing_record: record.is_empty(),
        choosing_organs: None,
        show_credits: false,
    };
    request(
        world,
        &mut state,
        Command::Context {
            record_uid: record.into(),
            data,
        },
    );
    world.entity_mut(panel).insert(state);
    draw(world, panel);
    panel
}

/// Mounts one side of a Record policy for editing without saving it.
/// Set `exclusions` to edit the exclusion conditions.
pub fn mount_policy_draft(
    world: &mut World,
    parent: Entity,
    mut context: Context,
    policy: Policy,
    exclusions: bool,
) -> Entity {
    context.saved = None;
    context.data = Data::Record;
    let entity = controls::column(world, parent);
    world.entity_mut(entity).insert(Panel {
        draft: Some(if exclusions {
            Side::Exclude
        } else {
            Side::Include
        }),
        owner: entity,
        record: context.record_uid.clone(),
        data: Data::Record,
        context: Some(context),
        include: policy.include.iter().map(Rule::from).collect(),
        exclude: policy.exclude.iter().map(Rule::from).collect(),
        prepared_policy: OnceLock::new(),
        fields: vec![],
        radius_fields: vec![],
        pending: None,
        notice: "Draft only; review all stages before enabling".into(),
        choosing_record: false,
        choosing_organs: None,
        show_credits: false,
    });
    draw(world, entity);
    entity
}

/// Reads and validates the current fields of a mounted policy editor.
///
/// # Errors
/// Returns a message if the editor is unavailable or its rules are invalid.
pub fn edited_policy(world: &mut World, entity: Entity) -> Result<Policy, String> {
    edited_policy_shared(world, entity).map(|policy| (*policy).clone())
}

pub(crate) fn edited_policy_shared(
    world: &mut World,
    entity: Entity,
) -> Result<Arc<Policy>, String> {
    let mut panel = world
        .entity_mut(entity)
        .take::<Panel>()
        .ok_or("Visibility draft unavailable")?;
    read_fields(world, &mut panel);
    let result = prepared_policy(&panel).clone();
    world.entity_mut(entity).insert(panel);
    result
}

pub fn set_target(world: &mut World, entity: Entity, record: &str, data: Data) {
    let Some(mut panel) = world.entity_mut(entity).take::<Panel>() else {
        return;
    };
    panel.record = record.into();
    panel.data = data;
    panel.prepared_policy.take();
    panel.context = None;
    panel.include.clear();
    panel.exclude.clear();
    panel.fields.clear();
    panel.radius_fields.clear();
    panel.choosing_record = record.is_empty();
    panel.choosing_organs = None;
    request(
        world,
        &mut panel,
        Command::Context {
            record_uid: record.into(),
            data,
        },
    );
    world.entity_mut(entity).insert(panel);
    draw(world, entity);
}

#[cfg(feature = "native-presentation")]
pub(crate) fn laboratory_cases(cases: &mut Vec<crate::native::laboratory::Case>) {
    tests::laboratory_cases(cases);
}
