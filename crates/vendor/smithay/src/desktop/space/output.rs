// non-lean-yet
// Scope: type OutputUserdata, fn set_output_location, fn output_location.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (17, 19, 23, 25, 35, 37).

use crate::{
    output::Output,
    utils::{Logical, Point},
};

use std::{collections::HashMap, sync::Mutex};

type OutputUserdata = Mutex<HashMap<usize, Point<i32, Logical>>>;

pub fn set_output_location(space: usize, o: &Output, new_loc: impl Into<Option<Point<i32, Logical>>>) {
    let userdata = o.user_data();
    userdata.insert_if_missing_threadsafe(OutputUserdata::default);

    match new_loc.into() {
        Some(loc) => userdata
            .get::<OutputUserdata>()
            .unwrap()
            .lock()
            .unwrap()
            .insert(space, loc),
        None => userdata
            .get::<OutputUserdata>()
            .unwrap()
            .lock()
            .unwrap()
            .remove(&space),
    };
}

pub fn output_location(space: usize, o: &Output) -> Point<i32, Logical> {
    let userdata = o.user_data();
    userdata.insert_if_missing_threadsafe(OutputUserdata::default);
    *userdata
        .get::<OutputUserdata>()
        .unwrap()
        .lock()
        .unwrap()
        .entry(space)
        .or_default()
}
