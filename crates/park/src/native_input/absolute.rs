use super::pointer::Target;
use bevy::{prelude::*, input::touch::TouchPhase, picking::pointer::Location};
use std::{collections::{HashMap, BTreeSet}, sync::{Arc, atomic::{AtomicU64, Ordering}}};
#[cfg(feature = "session")]
use std::sync::atomic::AtomicU8;
use lince_interface::host::input::TabletInput;

pub(crate) fn tablet_pointer(tool: u64) -> bevy::picking::pointer::PointerId {
    bevy::picking::pointer::PointerId::Custom(bevy::asset::uuid::Uuid::from_u128((0x4c494e434550454e_u128 << 64) | u128::from(tool)))
}

pub(crate) fn is_tablet_pointer(pointer: &bevy::picking::pointer::PointerId) -> bool {
    matches!(pointer, bevy::picking::pointer::PointerId::Custom(id) if id.as_u128() >> 64 == 0x4c494e434550454e)
}

#[derive(Default)]
#[cfg(feature = "session")]
pub struct FocusReceipt(AtomicU8);

#[cfg(feature = "session")]
impl FocusReceipt {
    pub(crate) fn pending(&self) -> bool { self.0.load(Ordering::Acquire) == 0 }
    pub(crate) fn accepted(&self) -> bool { self.0.load(Ordering::Acquire) == 1 }
    pub(crate) fn cancel(&self) { self.0.store(3, Ordering::Release); }
    pub(crate) fn complete(&self, accepted: bool) {
        let _ = self.0.compare_exchange(0, if accepted { 1 } else { 2 }, Ordering::AcqRel, Ordering::Acquire);
    }
}

#[cfg(feature = "session")]
pub enum FocusTarget {
    Touch(Touch),
    Tablet(Tablet),
    Application(lince_interface::host::SurfaceId),
    Layer,
}

#[cfg(feature = "session")]
pub struct Focus {
    pub(crate) output: lince_interface::host::OutputId,
    pub(crate) mapping: u64,
    pub(crate) target: FocusTarget,
    pub(crate) lifetime: Arc<AtomicU64>,
    pub(crate) epoch: u64,
    pub(crate) scope: Option<(Arc<AtomicU64>, u64)>,
    pub(crate) receipt: Arc<FocusReceipt>,
}

#[cfg(feature = "session")]
impl Focus {
    pub(crate) fn current(&self) -> bool {
        self.receipt.pending() && self.lifetime.load(Ordering::Acquire) == self.epoch
            && self.scope.as_ref().is_none_or(|(scope, epoch)| scope.load(Ordering::Acquire) == *epoch)
    }

    pub(crate) fn native(&self) -> bool { matches!(self.target, FocusTarget::Touch(_) | FocusTarget::Tablet(_)) }
}

#[derive(Clone)]
pub struct Touch {
    pub id: u64,
    pub phase: TouchPhase,
    pub point: [f64; 2],
    pub mapping: u64,
    pub lifetime: Arc<AtomicU64>,
    pub epoch: u64,
    pub target: Option<Target>,
}

impl Touch {
    pub(crate) fn current(&self) -> bool { self.lifetime.load(Ordering::Acquire) == self.epoch }
}

pub(crate) struct Contact {
    pub target: Target,
    pub location: Location,
    pub window: Entity,
    pub mapping: u64,
    pub pointer: Entity,
    pub lifetime: Arc<AtomicU64>,
    pub epoch: u64,
}

#[derive(Resource, Default)]
pub(crate) struct Contacts(pub HashMap<u64, Contact>);

#[derive(Clone)]
pub struct Tablet {
    pub event: TabletInput,
    pub mapping: u64,
    pub lifetime: Arc<AtomicU64>,
    pub epoch: u64,
    pub scope: Arc<AtomicU64>,
    pub scope_epoch: u64,
    pub held: bool,
    pub fresh_press: bool,
    pub target: Option<Target>,
}

impl Tablet {
    pub(crate) fn current(&self) -> bool { self.lifetime.load(Ordering::Acquire) == self.epoch && self.scope.load(Ordering::Acquire) == self.scope_epoch }
}

pub(crate) struct Pen {
    pub target: Target,
    pub location: Location,
    pub pointer: Entity,
    pub mapping: u64,
    pub lifetime: Arc<AtomicU64>,
    pub epoch: u64,
    pub scope: Arc<AtomicU64>,
    pub scope_epoch: u64,
    pub tip: bool,
    pub buttons: BTreeSet<u32>,
    pub last: TabletInput,
}

#[derive(Resource, Default)]
pub(crate) struct Pens {
    pub tools: HashMap<u64, Pen>,
    pub retired: Vec<Entity>,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Pens>().add_message::<TabletInput>().add_systems(Last, retire);
}

fn retire(world: &mut World) {
    let entities = std::mem::take(&mut world.resource_mut::<Pens>().retired);
    for entity in entities { if let Ok(entity) = world.get_entity_mut(entity) { entity.despawn(); } }
}
