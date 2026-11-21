// non-lean-yet

mod chemistry;
mod computing;
mod engineering;
mod inquiry;
mod mathematics;
mod physics;

use crate::ergon::fixture::{Edges, Records};

pub(super) use inquiry::LEARNING;

pub(super) fn extend_core(records: &Records<'_>, edges: &mut Edges) {
    for route in [
        chemistry::ROUTES,
        computing::ROUTES,
        engineering::ROUTES,
        inquiry::ROUTES,
        mathematics::ROUTES,
        physics::ROUTES,
    ]
    .into_iter()
    .flatten()
    {
        edges.extend(
            route
                .array_windows()
                .map(|[from, to]| (records[from].uid.clone(), records[to].uid.clone())),
        );
    }
}
