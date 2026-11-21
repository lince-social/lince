#![expect(
    clippy::panic_in_result_fn,
    reason = "tests assert behavior and propagate setup and mutation errors"
)]

use std::error::Error;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{from_slice, to_vec};

use super::{Member, Role, Room, identity_pose};
use crate::{
    canvas::{
        Component, Geometry, Metadata, Mutation, Placement, Pose, Receipt, Request, Response,
        Snapshot, State, Workspace,
    },
    component::ComponentState,
    new_uid,
};

mod geometry;
mod membership;
mod validation;

type TestResult = Result<(), Box<dyn Error>>;

fn placement() -> Placement {
    Placement {
        id: new_uid("placement"),
        workspace: 1,
        component: Component::Builtin {
            state: ComponentState::Record {
                record: new_uid("r"),
                mode: Default::default(),
                start_call: None,
            },
        },
        geometry: Geometry {
            position: [31.0, 47.0],
            size: [100.0; 2],
        },
        selected: false,
        group: None,
        metadata: Metadata::default(),
    }
}

fn attached(room: &str, parent: Option<&str>, role: Role, position: [f64; 3]) -> Placement {
    let mut item = placement();
    item.metadata.room_member = Some(Member {
        room: room.into(),
        parent: parent.map(str::to_owned),
        local: Pose {
            position,
            ..identity_pose()
        },
        role,
    });
    item
}

fn hierarchy() -> Vec<Placement> {
    let mut root = placement();
    root.metadata.room = Some(Room::default());
    let desk = attached(&root.id, None, Role::Desk, [0.0; 3]);
    let computer = attached(&root.id, Some(&desk.id), Role::Computer, [1.0, 2.0, 3.0]);
    let task = attached(&root.id, Some(&computer.id), Role::Content, [4.0, 5.0, 6.0]);
    vec![root, desk, computer, task]
}

fn room(placements: &[Placement]) -> &Placement {
    placements.first().expect("the fixture has a Room")
}

fn task(placements: &[Placement]) -> &Placement {
    placements
        .get(3)
        .expect("the fixture has a task under the computer")
}

fn member(metadata: &mut Metadata) -> &mut Member {
    metadata
        .room_member
        .as_mut()
        .expect("the fixture has Room membership")
}

fn state() -> Result<State, String> {
    State::new(
        Snapshot {
            revision: 0,
            active_workspace: 1,
            workspaces: vec![Workspace {
                id: 1,
                name: "Main".into(),
                center: [0.0; 2],
                zoom: 1.0,
                view: None,
            }],
            placements: hierarchy(),
            next_offset: None,
        },
        Vec::new(),
    )
}

fn mutate(state: &mut State, mutation: Mutation) -> Result<Receipt, String> {
    match state.handle(&Request::Mutate {
        request_id: new_uid("request"),
        expected_revision: state.snapshot.revision,
        mutation,
    })? {
        Response::Receipt { receipt } => Ok(receipt),
        _ => Err("Expected a Room mutation receipt.".into()),
    }
}

fn move_to(state: &mut State, placement: &str, position: [f64; 2]) -> Result<Receipt, String> {
    mutate(
        state,
        Mutation::Move {
            placement: placement.into(),
            workspace: 1,
            position,
        },
    )
}

fn undo(state: &mut State, receipt: Receipt) -> Result<Receipt, String> {
    mutate(
        state,
        Mutation::Undo {
            receipt: receipt.request_id,
        },
    )
}

fn undo_changes(state: &mut State, receipt: Receipt) -> Result<Receipt, String> {
    mutate(
        state,
        Mutation::UndoChanges {
            receipt: receipt.request_id,
        },
    )
}

fn roundtrip<T: Serialize + DeserializeOwned>(value: &T) -> serde_json::Result<T> {
    from_slice(&to_vec(value)?)
}
