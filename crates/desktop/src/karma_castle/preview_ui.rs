use bevy::{prelude::*, text::EditableText};
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
    pub(super) fn valid(&self) -> bool {
        self.horizon.len() <= 20
            && self.evaluations.len() <= 20
            && self.wall.len() <= 20
            && self.records.len() <= 2048
            && self.inputs.len() <= 65_536
            && self.saved_checks.len() <= 200
            && self.checks.len() <= 1024
            && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= 1024 * 1024)
    }
}

#[derive(Component)]
struct PreviewView {
    panel: Entity,
    output: Entity,
    pending: Option<(String, String, u64)>,
    report: Option<Report>,
    source_revision: u64,
    stale: bool,
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
        .insert(crate::token_style::border(crate::tokens::Token::TableGrid));
    {
        let mut node = world.get_mut::<Node>(panel).unwrap();
        node.display = Display::None;
        node.padding = UiRect::all(px(14));
        node.row_gap = px(12);
        node.border = UiRect::all(px(1));
    }
    crate::edit_mode::label(world, panel, "Simulate unsaved Rules", 16.0);
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
        crate::edit_mode::label(world, line, label, 13.0);
        let input = world
            .spawn(crate::sand::text_editor(
                &value,
                world.resource::<crate::theme::Typography>(),
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
    crate::castle_feed::button(world, line, owner, "Run unsaved Rules", Command::Run);
    crate::castle_feed::button(
        world,
        line,
        owner,
        "Use Simulation checks and limits",
        Command::Setup,
    );
    crate::castle_feed::button(world, line, owner, "Open Simulation", Command::Open);
    crate::edit_mode::label(
        world,
        panel,
        "The run uses copied data. Saving a Rule does not fire it: add an occurrence input or a quantity change when needed. Saving live edits is separate.",
        13.0,
    );
    let output = crate::edit_mode::label(world, panel, "", 13.0);
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
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .filter(|(input, _)| input.owner == owner)
        .map(|(input, text)| (input.index, text.value().to_string()))
        .collect();
    let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
    for (index, value) in values {
        let form = &mut castle.preview;
        match index {
            0 => form.horizon = value,
            1 => form.evaluations = value,
            2 => form.wall = value,
            3 => form.records = value,
            4 => form.inputs = value,
            5 => form.saved_checks = value,
            _ => {}
        }
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
            fields: draft.fields.clone().map(|field| field.input()),
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

impl crate::actions::Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        ui::capture(world, owner);
        capture(world, owner);
        if matches!(self, Self::Open) {
            let root = world.get::<ChildOf>(owner).unwrap().parent();
            crate::simulation_castle::open_current(world, root, None);
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
    let models: Vec<_> = world
        .query::<(&crate::simulation_castle::SimulationCastle, &ChildOf)>()
        .iter(world)
        .filter(|(_, parent)| parent.parent() == root)
        .map(|(model, _)| model.clone())
        .collect();
    if models.len() != 1 {
        return Err(
            "Open one Simulation Castle in this workspace to choose its checks and limits".into(),
        );
    }
    let scenario: simulation::scenario::Scenario =
        serde_json::from_str(&models[0].scenario).map_err(|error| error.to_string())?;
    let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
    let form = &mut castle.preview;
    form.horizon = (scenario.end_ms - scenario.start_ms).to_string();
    form.evaluations = scenario.limits.rule_evaluations.to_string();
    form.wall = scenario
        .limits
        .wall_time_ms
        .map_or_else(String::new, |value| value.to_string());
    form.checks = scenario.checks;
    form.checks_start_ms = Some(scenario.start_ms);
    form.checking = scenario.checking;
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
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<PreviewView>>()
        .iter(world)
        .collect();
    for owner in owners {
        capture(world, owner);
        let fingerprint = request(world, owner)
            .and_then(|request| request.fingerprint().map_err(|error| error.to_string()))
            .ok();
        let mut view = world.get_mut::<PreviewView>(owner).unwrap();
        if let Some(report) = &view.report {
            if fingerprint.as_deref() != Some(&report.draft) {
                view.stale = true;
            }
        }
        if view.stale && view.report.is_some() && view.pending.is_none() {
            let output = view.output;
            if !world
                .get::<Text>(output)
                .unwrap()
                .0
                .starts_with("Earlier draft or data · ")
            {
                let old = world.get::<Text>(output).unwrap().0.clone();
                world.get_mut::<Text>(output).unwrap().0 = format!("Earlier draft or data · {old}");
            }
        }
    }
}

pub(super) fn receive(world: &mut World, message: &ServerMessage) -> bool {
    let id = match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => id,
        _ => return false,
    };
    let owner = world
        .query::<(Entity, &PreviewView)>()
        .iter(world)
        .find(|(_, view)| {
            view.pending
                .as_ref()
                .is_some_and(|pending| &pending.0 == id)
        })
        .map(|(owner, _)| owner);
    let Some(owner) = owner else {
        return false;
    };
    ui::capture(world, owner);
    capture(world, owner);
    let fingerprint = request(world, owner)
        .and_then(|request| request.fingerprint().map_err(|error| error.to_string()))
        .ok();
    let names: HashMap<String, String> = world
        .get::<View>(owner)
        .into_iter()
        .flat_map(|view| {
            view.records
                .iter()
                .filter_map(|record| {
                    Some((
                        record["uid"].as_str()?.to_owned(),
                        record["head"].as_str()?.to_owned(),
                    ))
                })
                .chain(
                    view.rules
                        .iter()
                        .map(|rule| (rule.uid.clone(), rule.name.clone())),
                )
        })
        .collect();
    let mut view = world.get_mut::<PreviewView>(owner).unwrap();
    let submitted = view.pending.take().unwrap();
    let output = view.output;
    let text = match message {
        ServerMessage::ActionOk {
            data: Some(data), ..
        } => match serde_json::from_value::<Report>(data.clone()) {
            Ok(report) => {
                view.stale = !report.source_current
                    || submitted.2 != view.source_revision
                    || fingerprint.as_deref() != Some(&report.draft);
                let text = describe(&report, &names);
                view.report = Some(report);
                text
            }
            Err(error) => format!("Could not read Simulation result: {error}"),
        },
        ServerMessage::Error { message, .. } => message.clone(),
        _ => "Simulation returned no report".into(),
    };
    world.get_mut::<Text>(output).unwrap().0 = text;
    true
}

fn describe(report: &Report, names: &HashMap<String, String>) -> String {
    let name = |uid: &str| names.get(uid).cloned().unwrap_or_else(|| uid.into());
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
                    format!("{} level {}", name(person), participant.guard.level)
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "{} · revision {} · {} · {} · {} · {levels}",
                name(&value.record),
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
            name(&value.record),
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
                name(record.as_str()),
                expected.value,
                observed.value
            ),
            nucleus::simulation::Witness::RefusedRule { occurrence, .. } => {
                format!("{} was refused", name(&occurrence.rule_uid))
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
                .map(|rule| name(rule))
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
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins(KarmaCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::workspace::Workspaces::default())
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
