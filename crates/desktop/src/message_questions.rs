use crate::{actions::Action, protein_area::RecordBinding};
use bevy::{prelude::*, text::EditableText};
use nucleus::question::{Question, State};
use serde_json::{Value, json};

#[derive(Component)]
struct Builder {
    panel: Entity,
    prompt: Entity,
    responder: Entity,
    title: Entity,
    choices: Entity,
    list: Entity,
    status: Entity,
    fields: Vec<(String, Value)>,
    kind: String,
}

fn input(world: &mut World, parent: Entity, title: &str, initial: &str) -> Entity {
    crate::edit_mode::label(world, parent, title, 13.0);
    let input = world
        .spawn((
            crate::sand::text_editor(initial, world.resource::<crate::theme::Typography>(), 0),
            ChildOf(parent),
        ))
        .insert(Node {
            width: percent(100),
            max_height: px(100),
            ..default()
        })
        .id();
    world.get_mut::<EditableText>(input).unwrap().max_characters = Some(8192);
    input
}

fn text(world: &World, input: Entity) -> String {
    world
        .get::<EditableText>(input)
        .map(|text| text.value().to_string())
        .unwrap_or_default()
}

pub(crate) fn composer(world: &mut World, owner: Entity, parent: Entity) {
    crate::description::button(world, parent, owner, "Question form", Open);
    let panel = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    let prompt = input(world, panel, "Question", "");
    let responder = input(
        world,
        panel,
        "Who should answer? Use their Record's short name or identifier; me means yourself",
        "me",
    );
    let title = input(world, panel, "Field label", "");
    let choices = input(
        world,
        panel,
        "Choice options · one per line, only for choice fields",
        "",
    );
    let kinds = [
        ("Text", "string"),
        ("Number", "number"),
        ("Yes / no", "boolean"),
        ("One choice", "choice"),
        ("Several choices", "array"),
    ];
    crate::dropdown::spawn(
        world,
        panel,
        owner,
        "Field type",
        "Text",
        kinds
            .iter()
            .map(|(label, kind)| (label.to_string(), crate::actions![Kind(kind.to_string())]))
            .collect(),
    );
    crate::description::button(world, panel, owner, "Add required field", AddField);
    let list = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(panel),
        ))
        .id();
    let status =
        crate::edit_mode::label(world, panel, "Answers will be shared in this thread.", 12.0);
    crate::description::button(world, panel, owner, "Add question to draft", Add);
    world.entity_mut(owner).insert(Builder {
        panel,
        prompt,
        responder,
        title,
        choices,
        list,
        status,
        fields: Vec::new(),
        kind: "string".into(),
    });
}

#[derive(Clone)]
struct Open;
impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(panel) = world.get::<Builder>(owner).map(|builder| builder.panel) else {
            return;
        };
        let mut node = world.get_mut::<Node>(panel).unwrap();
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
    }
}

#[derive(Clone)]
struct Kind(String);
impl Action for Kind {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut builder) = world.get_mut::<Builder>(owner) {
            builder.kind = self.0.clone();
            let status = builder.status;
            world.get_mut::<Text>(status).unwrap().0 = format!("Next field type: {}", self.0);
        }
    }
}

#[derive(Clone)]
struct AddField;
impl Action for AddField {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(builder) = world.get::<Builder>(owner) else {
            return;
        };
        let title = text(world, builder.title);
        if title.trim().is_empty() || builder.fields.len() >= 32 {
            return;
        }
        let choices: Vec<_> = text(world, builder.choices)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        let mut field = json!({"type":builder.kind,"title":title});
        if builder.kind == "choice" {
            field["type"] = "string".into();
            field["enum"] = json!(choices);
        }
        if builder.kind == "array" {
            field["items"] = json!({"type":"string","enum":choices});
            field["minItems"] = 1.into();
        }
        if matches!(builder.kind.as_str(), "choice" | "array") && choices.is_empty() {
            return;
        }
        let name = (1..=33)
            .map(|index| format!("field{index}"))
            .find(|name| !builder.fields.iter().any(|(existing, _)| existing == name))
            .unwrap();
        world
            .get_mut::<Builder>(owner)
            .unwrap()
            .fields
            .push((name, field));
        render_fields(world, owner);
    }
}

fn render_fields(world: &mut World, owner: Entity) {
    let builder = world.get::<Builder>(owner).unwrap();
    let (list, fields) = (builder.list, builder.fields.clone());
    world.entity_mut(list).despawn_children();
    for (index, (_, field)) in fields.iter().enumerate() {
        crate::edit_mode::label(
            world,
            list,
            &format!(
                "{} · {}",
                field["title"].as_str().unwrap_or("Field"),
                field["type"].as_str().unwrap_or_default()
            ),
            13.0,
        );
        crate::description::button(world, list, owner, "Remove field", Remove(index));
    }
}

#[derive(Clone)]
struct Remove(usize);
impl Action for Remove {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(mut builder) = world.get_mut::<Builder>(owner) else {
            return;
        };
        if self.0 < builder.fields.len() {
            builder.fields.remove(self.0);
        }
        for (index, (name, _)) in builder.fields.iter_mut().enumerate() {
            *name = format!("field{}", index + 1);
        }
        render_fields(world, owner);
    }
}

#[derive(Clone)]
struct Add;
impl Action for Add {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(draft) = world.get::<crate::message_content::Draft>(owner) else {
            return;
        };
        if draft.locked {
            return;
        }
        let Some(builder) = world.get::<Builder>(owner) else {
            return;
        };
        let (panel, status) = (builder.panel, builder.status);
        let question = Question {
            prompt: text(world, builder.prompt),
            responder: text(world, builder.responder).trim().into(),
            schema: json!({"type":"object","properties":builder.fields.iter().cloned().collect::<serde_json::Map<_, _>>(),"required":builder.fields.iter().map(|(name, _)| name).collect::<Vec<_>>()}),
            state: State::Pending,
            answers: None,
            expires_ms: None,
        };
        let mut parts = draft.parts.clone();
        parts.push(nucleus::message::MessagePart::Question { question });
        if let Err(error) = nucleus::message::validate(&parts) {
            world.get_mut::<Text>(status).unwrap().0 = error;
            return;
        }
        world
            .get_mut::<crate::message_content::Draft>(owner)
            .unwrap()
            .parts = parts;
        world.get_mut::<Node>(panel).unwrap().display = Display::None;
        crate::message_content::render_draft(world, owner);
    }
}

#[derive(Component)]
struct Response {
    binding: RecordBinding,
    uid: String,
    content: Value,
    part: usize,
    form: Entity,
    status: Entity,
    pending: bool,
}

pub(crate) fn view(
    world: &mut World,
    parent: Entity,
    binding: &RecordBinding,
    data: &Value,
    part: usize,
    question: &Question,
) {
    let owner = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    crate::edit_mode::label(world, owner, &question.prompt, 16.0);
    crate::edit_mode::label(
        world,
        owner,
        &format!("For {} · {:?}", question.responder, question.state),
        12.0,
    );
    let expired = question
        .expires_ms
        .is_some_and(|expires| nucleus::operation::now_ms() >= expires);
    if question.state != State::Pending || expired {
        if expired && question.state == State::Pending {
            crate::edit_mode::label(
                world,
                owner,
                "This request expired. Ask for a new question.",
                13.0,
            );
        }
        if let Some(answers) = &question.answers {
            crate::edit_mode::label(world, owner, &answers.to_string(), 13.0);
        }
        return;
    }
    let form = match crate::question_form::create(world, owner, &question.schema, None) {
        Ok(form) => form,
        Err(error) => {
            crate::edit_mode::label(world, owner, &error, 13.0);
            return;
        }
    };
    let status = crate::edit_mode::label(
        world,
        owner,
        "Review your answers before submitting. They are shared with this thread.",
        12.0,
    );
    for (label, state) in [
        ("Submit answers", State::Answered),
        ("Decline", State::Declined),
        ("Cancel question", State::Cancelled),
    ] {
        crate::description::button(world, owner, owner, label, Submit(state));
    }
    world.entity_mut(owner).insert(Response {
        binding: binding.clone(),
        uid: data["uid"].as_str().unwrap_or_default().into(),
        content: data["content"].clone(),
        part,
        form,
        status,
        pending: false,
    });
}

#[derive(Clone)]
struct Submit(State);
impl Action for Submit {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(response) = world.get::<Response>(owner) else {
            return;
        };
        if response.pending {
            return;
        }
        let (binding, uid, status, part) = (
            response.binding.clone(),
            response.uid.clone(),
            response.status,
            response.part,
        );
        let mut content = response.content.clone();
        let answers = if self.0 == State::Answered {
            match crate::question_form::answers(world, response.form) {
                Ok(answers) => answers,
                Err(error) => {
                    world.get_mut::<Text>(status).unwrap().0 = error;
                    return;
                }
            }
        } else {
            Value::Null
        };
        content["parts"][part]["question"]["state"] = json!(self.0);
        content["parts"][part]["question"]["answers"] = answers;
        match crate::protein_area::execute(
            world,
            &binding,
            owner,
            engine::actions::Action::SetExtension {
                target: uid,
                namespace: "lince.message-content".into(),
                fds: content,
            },
        ) {
            Ok(()) => {
                world.get_mut::<Response>(owner).unwrap().pending = true;
                world.get_mut::<Text>(status).unwrap().0 = "Saving response…".into();
            }
            Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
        }
    }
}

pub(crate) fn finished(world: &mut World, owner: Entity, error: Option<String>) -> bool {
    let Some(mut response) = world.get_mut::<Response>(owner) else {
        return false;
    };
    response.pending = false;
    let status = response.status;
    world.get_mut::<Text>(status).unwrap().0 = error.unwrap_or_else(|| "Response saved".into());
    true
}
