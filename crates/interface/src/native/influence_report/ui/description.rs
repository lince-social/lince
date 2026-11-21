// non-lean-yet
// Scope: struct Scalar32, struct Scalar64, struct Vector, enum Reason.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 443.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (444, 452, 453).

use super::*;
use crate::native::{
    area::Direction,
    area_effects::{ForceMode, Immunity},
};
use std::{borrow::Cow, fmt};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Scalar32(u32);

impl fmt::Display for Scalar32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&f32::from_bits(self.0), f)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Scalar64(u64);

impl fmt::Display for Scalar64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&f64::from_bits(self.0), f)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Vector([Scalar64; 3]);

impl From<DVec3> for Vector {
    fn from(value: DVec3) -> Self {
        Self(value.to_array().map(|value| Scalar64(value.to_bits())))
    }
}

impl fmt::Display for Vector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:.2}, {:.2}, {:.2})", self.0[0], self.0[1], self.0[2])
    }
}

#[derive(PartialEq, Eq)]
enum Reason<'a> {
    Plain(&'static str),
    Immune(Cow<'a, str>),
}

impl<'a> Reason<'a> {
    fn new(world: &'a World, outcome: &Outcome) -> Self {
        Self::Plain(match outcome {
            Outcome::Active => "eligible",
            Outcome::Disabled => "Area disabled",
            Outcome::Paused => "paused",
            Outcome::Filter => "Record does not match the filter",
            Outcome::Reach => "outside influence reach",
            Outcome::Outside => "outside the Area boundary",
            Outcome::Immune(entity) => {
                return Self::Immune(
                    world
                        .get::<InfluenceArea>(*entity)
                        .map_or("removed Area", |area| area.name.as_str())
                        .into(),
                );
            }
            Outcome::Group => "Area and Sand belong to the same group",
            Outcome::Pinned => "Sand pinned to the screen",
        })
    }

    fn into_owned(self) -> Reason<'static> {
        match self {
            Self::Plain(reason) => Reason::Plain(reason),
            Self::Immune(name) => Reason::Immune(name.into_owned().into()),
        }
    }
}

impl fmt::Display for Reason<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain(reason) => f.write_str(reason),
            Self::Immune(name) => write!(f, "blocked by immunity from {name}"),
        }
    }
}

#[derive(PartialEq, Eq)]
struct Entry<'a> {
    name: Cow<'a, str>,
    direction: Direction,
    mode: ForceMode,
    motion: Reason<'a>,
    force: Vector,
    attraction: bool,
    sorting: Option<(bool, bool, Scalar64, Option<Vector>)>,
    size: Option<(Reason<'a>, Scalar32)>,
    immunity: Option<(Immunity, bool)>,
    boundary: Option<Cow<'a, str>>,
}

impl<'a> Entry<'a> {
    fn new(world: &'a World, sand: Entity, entry: &Evaluation) -> Option<Self> {
        let area = world.get::<InfluenceArea>(entry.area)?;
        Some(Self {
            name: area.name.as_str().into(),
            direction: area.direction,
            mode: area.force_mode,
            motion: Reason::new(world, &entry.motion),
            force: entry.force.into(),
            attraction: area.attraction_enabled,
            sorting: area.sorting.as_ref().map(|sort| {
                (
                    sort.horizontal,
                    sort.reverse,
                    Scalar64(sort.strength.to_bits()),
                    entry.slot.map(Vector::from),
                )
            }),
            size: (area.scale != 1.0).then(|| {
                (
                    Reason::new(world, &entry.size),
                    Scalar32(entry.scale.to_bits()),
                )
            }),
            immunity: (area.immunity != Immunity::None).then_some((area.immunity, entry.inside)),
            boundary: (!area.changes.is_empty())
                .then(|| crate::native::area_mutation::explanation(world, entry.area, sand).into()),
        })
    }

    fn into_owned(self) -> Entry<'static> {
        Entry {
            name: self.name.into_owned().into(),
            direction: self.direction,
            mode: self.mode,
            motion: self.motion.into_owned(),
            force: self.force,
            attraction: self.attraction,
            sorting: self.sorting,
            size: self
                .size
                .map(|(reason, scale)| (reason.into_owned(), scale)),
            immunity: self.immunity,
            boundary: self.boundary.map(|text| text.into_owned().into()),
        }
    }
}

impl fmt::Display for Entry<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {:?} / {:?}: {}. Force {}.",
            self.name, self.direction, self.mode, self.motion, self.force
        )?;
        if !self.attraction {
            f.write_str("\nAttraction/repulsion is switched off; sorting can still act.")?;
        }
        if let Some((horizontal, reverse, strength, slot)) = self.sorting {
            write!(
                f,
                "\nSorting: {}{}, strength {:.2}, ",
                if horizontal { "horizontal" } else { "vertical" },
                if reverse { ", reversed" } else { "" },
                strength
            )?;
            match slot {
                Some(slot) => write!(f, "slot at {slot}.")?,
                None => f.write_str("no slot for this Sand.")?,
            }
        }
        if let Some((reason, scale)) = &self.size {
            write!(f, "\nSize rule: {reason}; multiplier {scale:.2}.")?;
        }
        if let Some((immunity, inside)) = self.immunity {
            write!(
                f,
                "\nImmunity {immunity:?}; boundary {} this Sand. Other rules above name this Area when it blocks them.",
                if inside {
                    "contains"
                } else {
                    "does not contain"
                }
            )?;
        }
        if let Some(boundary) = &self.boundary {
            write!(f, "\n{boundary}")?;
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
enum Header<'a> {
    Message(&'static str),
    Report {
        condition: &'static str,
        force: Vector,
        scale: Scalar32,
        opposed: bool,
        capped: bool,
        empty: bool,
        error: Option<Cow<'a, str>>,
    },
}

impl Header<'_> {
    fn into_owned(self) -> Header<'static> {
        match self {
            Self::Message(message) => Header::Message(message),
            Self::Report {
                condition,
                force,
                scale,
                opposed,
                capped,
                empty,
                error,
            } => Header::Report {
                condition,
                force,
                scale,
                opposed,
                capped,
                empty,
                error: error.map(|error| error.into_owned().into()),
            },
        }
    }

    fn start(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::Report {
                condition,
                force,
                scale,
                opposed,
                capped,
                ..
            } => {
                write!(
                    f,
                    "Inspected Sand\n{condition}\nCombined force: {force}. Size multiplier: {scale:.2}."
                )?;
                if *opposed {
                    f.write_str(
                        "\nSome forces oppose one another. Pause an Area to isolate its effect.",
                    )?;
                }
                if *capped {
                    f.write_str("\nCombined force was limited to 1000000.")?;
                }
                f.write_str("\nSize stays between 0.05 and 20 times its base size. Boundary-change requests and repeated cycles have the limits below.")
            }
        }
    }

    fn finish(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::Report { empty, error, .. } = self {
            if *empty {
                f.write_str("\nNo valid Areas are present in this workspace.")?;
            }
            if let Some(error) = error {
                write!(f, "\n{error}")?;
            }
        }
        Ok(())
    }
}

fn condition(world: &World, root: Entity, sand: Entity, workspace: u64) -> &'static str {
    if world
        .get::<crate::native::sand_placement::Pinned>(sand)
        .is_some()
    {
        return "Pinned to the screen: forces, sorting, size and boundary changes are suppressed.";
    }
    if crate::native::topology::spatial(world, sand).world_pinned {
        return "Position pinned in space: forces cannot move this Sand; size rules still apply.";
    }
    let (mut held, mut pending) = (false, false);
    for member in crate::native::canvas_selection::group_members_iter(world, root, sand) {
        if member != sand
            && (world
                .get::<crate::native::sand_placement::Pinned>(member)
                .is_some()
                || crate::native::topology::spatial(world, member).world_pinned)
        {
            return "Another member of this Sand's group is pinned; the group stays fixed.";
        }
        held = held || crate::native::physics::held(world, member);
        if !held {
            pending = pending || crate::native::area_mutation::pending(world, member);
        }
    }
    if held {
        "This Sand or its group is being edited or placed; ordinary movement is temporarily held."
    } else if pending {
        "A Record change is pending; physics temporarily holds this Sand or its group."
    } else if crate::native::workspace_config::rules(world, root, workspace).paused {
        "Workspace rules are paused: Area effects and boundary changes are stopped."
    } else if !crate::native::workspace_config::enabled(world, root, workspace) {
        "Workspace physics is off: evaluated forces and sorting do not move this Sand."
    } else {
        "Workspace physics is on."
    }
}

pub(super) struct Description<'a> {
    world: &'a World,
    header: Header<'a>,
    source: Option<(Entity, &'a Report, Option<Entity>)>,
}

impl<'a> Description<'a> {
    pub(super) fn new(
        world: &'a World,
        root: Entity,
        sand: Option<Entity>,
        filter: Option<Entity>,
    ) -> Self {
        let mut description = Self {
            world,
            header: Header::Message("Select a Sand to inspect its evaluated rules."),
            source: None,
        };
        let Some(sand) = sand else { return description };
        let Some(report) = world.get::<Report>(sand) else {
            description.header = Header::Message("Waiting for rule evaluation.");
            return description;
        };
        let workspace = world
            .get::<WorkspaceMember>(sand)
            .map_or(1, |member| member.0);
        let raw: DVec3 = report.entries.iter().map(|entry| entry.force).sum();
        let magnitudes: f64 = report
            .entries
            .iter()
            .map(|entry| entry.force.length())
            .sum();
        let length = raw.length();
        description.header = Header::Report {
            condition: condition(world, root, sand, workspace),
            force: report.total.into(),
            scale: Scalar32(report.scale.to_bits()),
            opposed: magnitudes > 0.0 && length < magnitudes * 0.5,
            capped: length > 1_000_000.0,
            empty: report.entries.is_empty(),
            error: world
                .get::<crate::native::workspace_config::WorkspaceSettings>(root)
                .and_then(|settings| settings.0.get(&workspace))
                .and_then(|setting| setting.error.as_deref())
                .map(Cow::Borrowed),
        };
        description.source = Some((sand, report, filter));
        description
    }

    fn entries(&self) -> impl Iterator<Item = Entry<'a>> + 'a + use<'a> {
        let world = self.world;
        self.source
            .into_iter()
            .flat_map(move |(sand, report, filter)| {
                report
                    .entries
                    .iter()
                    .filter(move |entry| filter.is_none_or(|area| area == entry.area))
                    .filter_map(move |entry| Entry::new(world, sand, entry))
            })
    }

    fn matches(&self, snapshot: &Snapshot) -> bool {
        if self.header != snapshot.header {
            return false;
        }
        let mut previous = snapshot.entries.iter();
        for entry in self.entries() {
            if previous.next().is_none_or(|old| entry != *old) {
                return false;
            }
        }
        previous.next().is_none()
    }

    fn into_snapshot(self) -> Snapshot {
        let entries = self.entries().map(Entry::into_owned).collect();
        Snapshot {
            header: self.header.into_owned(),
            entries,
        }
    }
}

impl fmt::Display for Description<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.header.start(f)?;
        for entry in self.entries() {
            write!(f, "\n{entry}")?;
        }
        self.header.finish(f)
    }
}

struct Snapshot {
    header: Header<'static>,
    entries: Vec<Entry<'static>>,
}

impl fmt::Display for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.header.start(f)?;
        for entry in &self.entries {
            write!(f, "\n{entry}")?;
        }
        self.header.finish(f)
    }
}

#[derive(Component)]
struct Presented {
    snapshot: Snapshot,
    text: String,
}

pub(super) fn update(
    world: &mut World,
    text: Entity,
    root: Entity,
    sand: Option<Entity>,
    filter: Option<Entity>,
) {
    let _description = nucleus::performance::Span::new("influence-description-presentation");
    let Some(label) = world.get::<Text>(text) else {
        return;
    };
    let current = Description::new(world, root, sand, filter);
    if let Some(previous) = world
        .get::<Presented>(text)
        .filter(|old| current.matches(&old.snapshot))
    {
        nucleus::performance::add("influence-descriptions-retained", 1);
        if label.0 != previous.text {
            let value = previous.text.clone();
            world.get_mut::<Text>(text).unwrap().0 = value;
            nucleus::performance::add("influence-description-labels-published", 1);
        }
        return;
    }
    let snapshot = current.into_snapshot();
    let value = snapshot.to_string();
    nucleus::performance::add("influence-descriptions-formatted", 1);
    if world.get::<Text>(text).unwrap().0 != value {
        value.clone_into(&mut world.get_mut::<Text>(text).unwrap().0);
        nucleus::performance::add("influence-description-labels-published", 1);
    }
    world.entity_mut(text).insert(Presented {
        snapshot,
        text: value,
    });
}
