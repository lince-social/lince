// non-lean-yet
// Scope: struct Form, fn run_action, fn confirmed, fn needs_refresh.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 136, 306, 307,
//   309, 343, 347, 386, 402, 404, 408, 409, 410, 442, 581, 700.
// - [review: §3 Ownership] Check that these inputs can be borrowed without changing ownership
//   semantics. Sites with_inputs:inputs → &[T] (46).
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 318.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (226, 463,
//   479).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   423.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (51, 129, 136, 158, 177, 183, 215, 235, 361, 371, 372, 378, 394, 421, 425, 426,
//   432, 446, 449, 492, 493, 508, 564, 565, 585).

use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
    text::EditableText,
};
use engine::karma_preview::{Limits, ProposedRule, Report, Request};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Form {
    horizon: String,
    evaluations: String,
    wall: String,
    records: String,
    inputs: String,
    saved_checks: String,
    checks: Vec<nucleus::simulation::CheckDefinition>,
    checks_start_ms: Option<i64>,
    checking: nucleus::simulation::Checking,
    steps: u64,
    evidence_bytes: u64,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            horizon: "1d".into(),
            evaluations: "100000".into(),
            wall: "1m".into(),
            records: String::new(),
            inputs: "[]".into(),
            saved_checks: String::new(),
            checks: Vec::new(),
            checks_start_ms: None,
            checking: Default::default(),
            steps: Limits::default().steps,
            evidence_bytes: Limits::default().evidence_bytes,
        }
    }
}

impl Form {
    pub(crate) fn with_inputs(inputs: Vec<engine::karma_preview::Input>) -> Self {
        Self {
            horizon: "1m".into(),
            wall: "5s".into(),
            evaluations: "100".into(),
            inputs: serde_json::to_string(&inputs).unwrap(),
            ..default()
        }
    }
    pub(super) fn valid(&self) -> bool {
        self.horizon.len() <= 20
            && self.evaluations.len() <= 20
            && self.wall.len() <= 20
            && self.records.len() <= 2048
            && self.inputs.len() <= 65_536
            && self.saved_checks.len() <= 200
            && self.checks.len() <= 1024
            && nucleus::json::encoded_len(self).is_ok_and(|bytes| bytes <= 1024 * 1024)
    }
}

pub(super) fn run_action(world: &mut World, owner: Entity) {
    Command::Run.apply(world, owner);
}

pub(super) fn confirmed(world: &World, owner: Entity) -> bool {
    world.get::<PreviewView>(owner).is_some_and(|view| {
        !view.stale
            && view.pending.is_none()
            && view.report.as_ref().is_some_and(|report| {
                report.first_failure.is_none()
                    && !report.incomplete
                    && report.unsupported.is_empty()
            })
    })
}

pub(super) fn needs_refresh(world: &World, owner: Entity) -> bool {
    world
        .get::<PreviewView>(owner)
        .is_some_and(|view| view.stale && view.pending.is_none() && view.report.is_some())
}

#[derive(Component)]
pub(super) struct PreviewView {
    panel: Entity,
    output: Entity,
    pending: Option<(String, String, u64)>,
    report: Option<Report>,
    source_revision: u64,
    stale: bool,
}

impl replies::Pending for PreviewView {
    fn pending_id(&self) -> Option<&str> {
        self.pending.as_ref().map(|pending| pending.0.as_str())
    }
}

#[derive(Component)]
struct Input {
    owner: Entity,
    index: usize,
}

#[derive(Clone, Copy)]
enum Command {
    Run,
    Setup,
    Open,
}

pub(super) fn spawn(world: &mut World, owner: Entity, parent: Entity) {
    let panel = ui::stack(world, parent);
    world
        .entity_mut(panel)
        .insert(Name::new("Karma simulation"));
    world
        .entity_mut(panel)
        .insert(crate::native::token_style::border(
            crate::native::tokens::Token::TableGrid,
        ));
    {
        let mut node = world.get_mut::<Node>(panel).unwrap();
        node.display = Display::None;
        node.padding = UiRect::all(px(14));
        node.row_gap = px(12);
        node.border = UiRect::all(px(1));
    }
    crate::native::edit_mode::label(world, panel, "Simulate unsaved Rules", 16.0);
    let form = world.get::<KarmaCastle>(owner).unwrap().preview.clone();
    for (index, label, value) in [
        (
            0,
            "Simulated span (1d or 100y; a year is 365 days)",
            form.horizon,
        ),
        (1, "Maximum Rule evaluations", form.evaluations),
        (
            2,
            "Real running span (for example 1m; empty = no limit)",
            form.wall,
        ),
        (
            3,
            "Final values: comma-separated Records (empty = Rule targets)",
            form.records,
        ),
        (4, "Controlled inputs", form.inputs),
        (5, "Saved check set ID (optional)", form.saved_checks),
    ] {
        let line = ui::stack(world, panel);
        world.get_mut::<Node>(line).unwrap().row_gap = px(4);
        crate::native::edit_mode::label(world, line, label, 13.0);
        let input = world
            .spawn(crate::native::sand::text_editor(
                &value,
                world.resource::<crate::native::theme::Typography>(),
                0,
            ))
            .insert((
                ChildOf(line),
                Input { owner, index },
                Node {
                    width: percent(100),
                    max_width: px(640),
                    min_width: px(0),
                    ..default()
                },
            ))
            .id();
        let mut text = world.get_mut::<EditableText>(input).unwrap();
        text.allow_newlines = index == 4;
        text.visible_lines = Some(if index == 4 { 3.0 } else { 1.0 });
        text.max_characters = Some(if index == 4 { 65_536 } else { 2048 });
    }
    let line = ui::row(world, panel);
    world.get_mut::<Node>(line).unwrap().flex_wrap = FlexWrap::Wrap;
    crate::native::castle_feed::button(world, line, owner, "Run unsaved Rules", Command::Run);
    crate::native::castle_feed::button(
        world,
        line,
        owner,
        "Use Simulation checks and limits",
        Command::Setup,
    );
    crate::native::castle_feed::button(world, line, owner, "Open Simulation", Command::Open);
    crate::native::edit_mode::label(
        world,
        panel,
        "The run uses copied data. Saving a Rule does not fire it: add an occurrence input or a quantity change when needed. Saving live edits is separate.",
        13.0,
    );
    let output = crate::native::edit_mode::label(world, panel, "", 13.0);
    world.entity_mut(owner).insert(PreviewView {
        panel,
        output,
        pending: None,
        report: None,
        source_revision: 0,
        stale: false,
    });
}

pub(super) fn toggle(world: &mut World, owner: Entity) {
    let Some(view) = world.get::<PreviewView>(owner) else {
        return;
    };
    let panel = view.panel;
    let mut node = world.get_mut::<Node>(panel).unwrap();
    node.display = if node.display == Display::None {
        Display::Flex
    } else {
        Display::None
    };
}

fn capture(world: &mut World, owner: Entity) {
    world
        .run_system_cached_with(capture_fields, owner)
        .expect("capture Karma preview fields");
}

fn capture_fields(
    In(owner): In<Entity>,
    inputs: Query<(&Input, &EditableText)>,
    mut castles: Query<&mut KarmaCastle, Allow<Disabled>>,
) {
    let _capture = nucleus::performance::Span::new("karma-preview-field-capture");
    let mut castle = castles.get_mut(owner).unwrap();
    for (input, text) in &inputs {
        if input.owner != owner {
            continue;
        }
        let form = &castle.preview;
        let current = match input.index {
            0 => &form.horizon,
            1 => &form.evaluations,
            2 => &form.wall,
            3 => &form.records,
            4 => &form.inputs,
            5 => &form.saved_checks,
            _ => continue,
        };
        if text.value() == current.as_str() {
            continue;
        }
        let form = &mut castle.preview;
        let target = match input.index {
            0 => &mut form.horizon,
            1 => &mut form.evaluations,
            2 => &mut form.wall,
            3 => &mut form.records,
            4 => &mut form.inputs,
            5 => &mut form.saved_checks,
            _ => unreachable!(),
        };
        crate::controls::copy_value(text, target);
        nucleus::performance::add("karma-preview-fields-published", 1);
    }
}

fn duration(value: &str) -> Result<u64, String> {
    let value = value.trim();
    for (suffix, multiplier) in [
        ("ms", 1),
        ("s", 1_000),
        ("m", 60_000),
        ("h", 3_600_000),
        ("d", 86_400_000),
        ("w", 604_800_000),
        ("y", 31_536_000_000),
        ("", 1),
    ] {
        if let Some(number) = value.strip_suffix(suffix) {
            return number
                .parse::<u64>()
                .ok()
                .and_then(|number| number.checked_mul(multiplier))
                .ok_or_else(|| "Use a whole span such as 1d, 100y, 1m or 500ms".into());
        }
    }
    Err("Use a whole span such as 1d, 100y, 1m or 500ms".into())
}

fn request(world: &World, owner: Entity) -> Result<Request, String> {
    let castle = world
        .get::<KarmaCastle>(owner)
        .ok_or("Rule editor unavailable")?;
    request_for(castle)
}

fn request_for(castle: &KarmaCastle) -> Result<Request, String> {
    let form = &castle.preview;
    let proposals = castle
        .draft
        .iter()
        .chain(castle.edits.iter())
        .map(|draft| ProposedRule {
            identity: Some(nucleus::karma::rule_field::RuleIdentity {
                name: draft.name.clone(),
                slug: draft.slug.clone(),
            }),
            rule: draft.rule.clone(),
            expected_revision: draft.revision,
            fields: draft.fields.each_ref().map(model::FieldDraft::input),
        })
        .collect();
    let parse = |value: &str| {
        value
            .trim()
            .parse::<u64>()
            .map_err(|_| "Enter a whole number for each Simulation limit".to_owned())
    };
    let request = Request {
        proposals,
        limits: Limits {
            horizon_ms: duration(&form.horizon)?,
            rule_evaluations: parse(&form.evaluations)?,
            wall_time_ms: if form.wall.trim().is_empty() {
                None
            } else {
                Some(duration(&form.wall)?)
            },
            steps: form.steps,
            evidence_bytes: form.evidence_bytes,
        },
        inputs: serde_json::from_str(&form.inputs)
            .map_err(|error| format!("Invalid controlled inputs: {error}"))?,
        records: form
            .records
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.trim_start_matches('@').to_owned())
            .collect(),
        quantity_basis: Default::default(),
        checks: form.checks.clone(),
        saved_checks: (!form.saved_checks.trim().is_empty())
            .then(|| form.saved_checks.trim().into()),
        checks_start_ms: form.checks_start_ms,
        checking: form.checking.clone(),
    };
    request.validate().map_err(|error| error.to_string())?;
    Ok(request)
}

impl crate::native::actions::Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::native::laboratory::suspended(world, owner) {
            return;
        }
        ui::capture(world, owner);
        capture(world, owner);
        if matches!(self, Self::Open) {
            let root = world.get::<ChildOf>(owner).unwrap().parent();
            crate::native::simulation_castle::open_current(world, root, None);
            return;
        }
        let result = if matches!(self, Self::Setup) {
            setup(world, owner)
        } else {
            run(world, owner)
        };
        if let Err(error) = result {
            let output = world.get::<PreviewView>(owner).unwrap().output;
            world.get_mut::<Text>(output).unwrap().0 = error;
        }
    }
}

fn setup(world: &mut World, owner: Entity) -> Result<(), String> {
    let root = world.get::<ChildOf>(owner).unwrap().parent();
    let sources: Vec<_> = world
        .query::<(
            &crate::native::simulation_castle::SimulationCastle,
            &ChildOf,
        )>()
        .iter(world)
        .filter(|(_, parent)| parent.parent() == root)
        .map(|(model, _)| model.scenario.clone())
        .collect();
    if sources.len() != 1 {
        return Err(
            "Open one Simulation Castle in this workspace to choose its checks and limits".into(),
        );
    }
    let scenario = sources[0].parsed()?;
    let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
    let form = &mut castle.preview;
    form.horizon = (scenario.end_ms - scenario.start_ms).to_string();
    form.evaluations = scenario.limits.rule_evaluations.to_string();
    form.wall = scenario
        .limits
        .wall_time_ms
        .map_or_else(String::new, |value| value.to_string());
    form.checks = scenario.checks.clone();
    form.checks_start_ms = Some(scenario.start_ms);
    form.checking = scenario.checking.clone();
    form.steps = scenario.limits.steps;
    form.evidence_bytes = scenario.limits.evidence_bytes;
    let values = [
        form.horizon.clone(),
        form.evaluations.clone(),
        form.wall.clone(),
    ];
    let inputs: Vec<_> = world
        .query::<(Entity, &Input)>()
        .iter(world)
        .filter(|(_, input)| input.owner == owner && input.index < 3)
        .map(|(entity, input)| (entity, input.index))
        .collect();
    for (entity, index) in inputs {
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(&values[index]);
    }
    let output = world.get::<PreviewView>(owner).unwrap().output;
    world.get_mut::<Text>(output).unwrap().0 =
        "Simulation checks and limits selected for this draft".into();
    Ok(())
}

fn run(world: &mut World, owner: Entity) -> Result<(), String> {
    if world.get::<PreviewView>(owner).unwrap().pending.is_some() {
        return Err("Simulation is still running".into());
    }
    let request = request(world, owner)?;
    let fingerprint = request.fingerprint().map_err(|error| error.to_string())?;
    let id = nucleus::new_uid("karma-preview");
    send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action: engine::actions::Action::PreviewKarmaProposal { request },
        },
    )?;
    let mut view = world.get_mut::<PreviewView>(owner).unwrap();
    view.pending = Some((id, fingerprint, view.source_revision));
    let output = view.output;
    world.get_mut::<Text>(output).unwrap().0 = "Simulating the captured unsaved Rules…".into();
    Ok(())
}

pub(super) fn dirty(world: &mut World, owner: Entity) {
    if let Some(mut view) = world.get_mut::<PreviewView>(owner) {
        view.source_revision = view.source_revision.wrapping_add(1);
        view.stale = true;
    }
}

pub(super) fn maintain(world: &mut World) {
    world
        .run_system_cached(maintain_previews)
        .expect("maintain Karma previews");
}

fn maintain_previews(
    world: &mut World,
    candidates: &mut QueryState<Entity, With<PreviewView>>,
    mut owners: Local<Vec<Entity>>,
) {
    let _maintain = nucleus::performance::Span::new("karma-preview-maintenance");
    owners.extend(candidates.iter(world));
    let count = owners.len();
    for owner in owners.drain(..) {
        capture(world, owner);
    }
    world
        .run_system_cached(maintain_reports)
        .expect("maintain Karma preview reports");
    if count == 0 {
        *owners = Vec::new();
    }
}

fn maintain_reports(
    owners: Query<Entity, With<PreviewView>>,
    castles: Query<Ref<KarmaCastle>, Allow<Disabled>>,
    mut views: Query<&mut PreviewView, Allow<Disabled>>,
    mut texts: Query<&mut Text, Allow<Disabled>>,
) {
    for owner in &owners {
        let castle = castles.get(owner).unwrap();
        let mut view = views.get_mut(owner).unwrap();
        if !view.stale
            && (castle.is_changed() || view.is_changed())
            && let Some(report) = &view.report
        {
            let _prepare = nucleus::performance::Span::new("karma-preview-fingerprint");
            nucleus::performance::add("karma-preview-fingerprints-prepared", 1);
            let fingerprint = request_for(&castle)
                .and_then(|request| request.fingerprint().map_err(|error| error.to_string()))
                .ok();
            if fingerprint.as_deref() != Some(&report.draft) {
                view.stale = true;
            }
        }
        if view.stale && view.report.is_some() && view.pending.is_none() {
            let mut text = texts.get_mut(view.output).unwrap();
            if !text.0.starts_with("Earlier draft or data · ") {
                text.0 = format!("Earlier draft or data · {}", text.0);
            }
        }
    }
}

struct PreparedReport {
    report: Report,
    text: String,
    fingerprint: Option<String>,
}

fn prepare_report(
    world: &World,
    owner: Entity,
    data: &Value,
) -> Result<PreparedReport, serde_json::Error> {
    let _prepare = nucleus::performance::Span::new("karma-preview-reply-preparation");
    let report = Report::deserialize(data)?;
    let fingerprint = request(world, owner)
        .and_then(|request| request.fingerprint().map_err(|error| error.to_string()))
        .ok();
    let names: HashMap<&str, &str> = world
        .get::<View>(owner)
        .into_iter()
        .flat_map(|view| {
            view.records
                .iter()
                .filter_map(|record| Some((record["uid"].as_str()?, record["head"].as_str()?)))
                .chain(
                    view.rules
                        .iter()
                        .map(|rule| (rule.uid.as_str(), rule.name.as_str())),
                )
        })
        .collect();
    nucleus::performance::add("karma-preview-names-borrowed", names.len() as u64);
    let text = describe(&report, &names);
    Ok(PreparedReport {
        report,
        text,
        fingerprint,
    })
}

pub(super) fn receive(world: &mut World, owner: Entity, message: &ServerMessage) {
    ui::capture(world, owner);
    capture(world, owner);
    let prepared = match message {
        ServerMessage::ActionOk {
            data: Some(data), ..
        } => Some(prepare_report(world, owner, data)),
        _ => None,
    };
    let mut view = world.get_mut::<PreviewView>(owner).unwrap();
    let submitted = view.pending.take().unwrap();
    let output = view.output;
    let text = match prepared {
        Some(Ok(PreparedReport {
            report,
            text,
            fingerprint,
        })) => {
            view.stale = !report.source_current
                || submitted.2 != view.source_revision
                || fingerprint.as_deref() != Some(&report.draft);
            view.report = Some(report);
            text
        }
        Some(Err(error)) => format!("Could not read Simulation result: {error}"),
        None => match message {
            ServerMessage::Error { message, .. } => message.clone(),
            _ => "Simulation returned no report".into(),
        },
    };
    world.get_mut::<Text>(output).unwrap().0 = text;
}

fn describe(report: &Report, names: &HashMap<&str, &str>) -> String {
    fn name<'a>(names: &HashMap<&str, &'a str>, uid: &'a str) -> &'a str {
        names.get(uid).copied().unwrap_or(uid)
    }
    let at = chrono::DateTime::from_timestamp_millis(report.stopped_at_ms).map_or_else(
        || report.stopped_at_ms.to_string(),
        |date| date.to_rfc3339(),
    );
    let reason = match &report.stop {
        nucleus::simulation::Stop::HorizonReached {} => "Reached the chosen end",
        nucleus::simulation::Stop::RuleEvaluationBudget {} => "Reached the Rule evaluation limit",
        nucleus::simulation::Stop::WallTimeBudget {} => "Reached the running time limit",
        nucleus::simulation::Stop::CheckFailed { .. } => "Stopped at a broken restriction",
        nucleus::simulation::Stop::UnsupportedEffect { .. } => {
            "Stopped before an unsupported action"
        }
        nucleus::simulation::Stop::Cancelled {} => "Stopped by request",
        nucleus::simulation::Stop::ExecutionError { .. } => {
            "Stopped after a refused or failed action"
        }
        _ => "Stopped at a Simulation limit",
    };
    let mut lines = vec![format!(
        "{reason} at {at} · {} evaluations{}",
        report.evaluations,
        if report.incomplete {
            " · incomplete coverage"
        } else {
            ""
        }
    )];
    for value in &report.final_values {
        if let Some(transfer) = &value.transfer {
            let levels = transfer
                .participants
                .iter()
                .map(|(person, participant)| {
                    format!("{} level {}", name(names, person), participant.guard.level)
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "{} · revision {} · {} · {} · {} · {levels}",
                name(names, &value.record),
                transfer.revision,
                if transfer.active {
                    "active"
                } else {
                    "inactive"
                },
                if transfer.published {
                    "published"
                } else {
                    "unpublished"
                },
                if transfer.ready { "ready" } else { "not ready" }
            ));
            continue;
        }
        lines.push(format!(
            "{} = {}",
            name(names, &value.record),
            value.quantity.as_ref().map_or_else(
                || "Unavailable".into(),
                |quantity| quantity.value.to_string()
            )
        ));
    }
    if let Some(finding) = &report.first_failure {
        let label = if finding.check.options.name.is_empty() {
            &finding.check.id
        } else {
            &finding.check.options.name
        };
        let detail = match &finding.witness {
            nucleus::simulation::Witness::Quantity {
                record,
                expected,
                observed,
            } => format!(
                "{}: expected {}, found {}",
                name(names, record.as_str()),
                expected.value,
                observed.value
            ),
            nucleus::simulation::Witness::RefusedRule { occurrence, .. } => {
                format!("{} was refused", name(names, &occurrence.rule_uid))
            }
            _ => "See the restriction and contributing Rules".into(),
        };
        lines.push(format!(
            "First broken restriction: {label} at {} · {detail}",
            finding.virtual_ms
        ));
    }
    for cycle in &report.cycles {
        let kind = match cycle.kind {
            nucleus::simulation::CycleKind::Feedback => "Feedback cycle",
            nucleus::simulation::CycleKind::SettledFeedback => "Feedback that settled",
            nucleus::simulation::CycleKind::TimedRecurrence => "Recurring sequence",
        };
        lines.push(format!(
            "{kind}: {}",
            cycle
                .rules
                .iter()
                .map(|rule| name(names, rule))
                .collect::<Vec<_>>()
                .join(" → ")
        ));
    }
    for unsupported in &report.unsupported {
        lines.push(unsupported.clone());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_preview_captures_unsaved_zero_and_marks_a_changed_draft_stale() {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins(KarmaCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::native::workspace::Workspaces::default())
            .id();
        let owner = super::super::spawn(
            app.world_mut(),
            root,
            1,
            DVec2::ZERO,
            KarmaCastle {
                draft: Some(Draft {
                    name: "Clean".into(),
                    slug: "clean".into(),
                    fields: ["1", "!=0", "@room = 0"].map(|source| model::FieldDraft {
                        text: source.into(),
                        linked: None,
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let panel = app.world().get::<PreviewView>(owner).unwrap().panel;
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::None
        );
        toggle(app.world_mut(), owner);
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::Flex
        );
        toggle(app.world_mut(), owner);
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::None
        );
        assert!(
            app.world()
                .get::<PreviewView>(owner)
                .unwrap()
                .pending
                .is_none()
        );
        let proposed = request(app.world(), owner).unwrap();
        assert!(
            matches!(&proposed.proposals[0].fields[2], nucleus::karma::rule_field::RuleFieldInput::Text { source } if source == "@room = 0")
        );
        let draft = proposed.fingerprint().unwrap();
        app.world_mut()
            .get_mut::<PreviewView>(owner)
            .unwrap()
            .report = Some(Report {
            required_reads: Vec::new(),
            draft,
            source: "source".into(),
            source_current: true,
            start_ms: 0,
            requested_until_ms: 1,
            stopped_at_ms: 1,
            stop: nucleus::simulation::Stop::HorizonReached {},
            evaluations: 1,
            final_values: Vec::new(),
            first_failure: None,
            cycles: Vec::new(),
            coverage: Vec::new(),
            incomplete: false,
            unsupported: Vec::new(),
            assumptions: Vec::new(),
        });
        app.world_mut()
            .get_mut::<KarmaCastle>(owner)
            .unwrap()
            .draft
            .as_mut()
            .unwrap()
            .fields[2]
            .text = "@room = -1".into();
        maintain(app.world_mut());
        assert!(app.world().get::<PreviewView>(owner).unwrap().stale);
        assert!(
            app.world()
                .get::<KarmaCastle>(owner)
                .unwrap()
                .draft
                .is_some()
        );
    }
}
