use super::*;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceRef<'a> {
    Local,
    Organ(&'a str),
}

impl<'a> From<&'a Source> for SourceRef<'a> {
    fn from(source: &'a Source) -> Self {
        match source {
            Source::Local => Self::Local,
            Source::Organ(uid) => Self::Organ(uid),
        }
    }
}

fn target<'a>(context: &'a Context, source: &'a NumericSource) -> Option<(SourceRef<'a>, &'a str)> {
    let record = source.record.as_deref().or(context.record.as_deref())?;
    let source = source
        .organ
        .as_deref()
        .map_or_else(|| SourceRef::from(&context.source), SourceRef::Organ);
    if !nucleus::valid_uid(record, "r")
        || matches!(source, SourceRef::Organ(uid) if !nucleus::valid_uid(uid, "r"))
    {
        return None;
    }
    Some((source, record))
}

struct Target {
    source: Source,
    group: String,
    key: String,
    request: protein::shader_inputs::Request,
    field_key: String,
}

pub(super) struct Input {
    pub(super) bindings: Vec<NumericBinding>,
    targets: Vec<Option<Target>>,
}

impl Input {
    fn new(context: &Context, bindings: &[NumericBinding]) -> Option<Self> {
        model::bindings::validate(bindings).ok()?;
        nucleus::performance::add("shader-numeric-inputs-prepared", 1);
        let targets = bindings
            .iter()
            .map(|binding| {
                let (source, record) = target(context, &binding.source)?;
                let source = match source {
                    SourceRef::Local => Source::Local,
                    SourceRef::Organ(uid) => Source::Organ(uid.to_owned()),
                };
                Some(Target {
                    group: source_key(&source),
                    source,
                    key: serde_json::to_string(&(record, &binding.source.field)).unwrap(),
                    request: protein::shader_inputs::Request {
                        record: record.to_owned(),
                        field: binding.source.field.clone(),
                    },
                    field_key: protein::shader_inputs::key(&binding.source.field),
                })
            })
            .collect();
        Some(Self {
            bindings: bindings.to_vec(),
            targets,
        })
    }

    fn matches(&self, context: &Context, bindings: &[NumericBinding]) -> bool {
        self.bindings == bindings
            && self
                .targets
                .iter()
                .zip(bindings)
                .all(|(prepared, binding)| {
                    let current = target(context, &binding.source);
                    match (prepared, current) {
                        (None, None) => true,
                        (Some(prepared), Some((source, record))) => {
                            SourceRef::from(&prepared.source) == source
                                && prepared.request.record == record
                        }
                        _ => false,
                    }
                })
    }

    pub(super) fn values<'a>(
        &self,
        feeds: &'a NumericFeeds,
        entity: Entity,
    ) -> [Option<&'a str>; 8] {
        let mut values = [None; 8];
        for (binding, target) in self.bindings.iter().zip(&self.targets) {
            if feeds.limited.contains(&(entity, binding.slot)) {
                continue;
            }
            let Some(target) = target else {
                continue;
            };
            values[usize::from(binding.slot)] = feeds
                .batches
                .get(&target.group)
                .and_then(|batch| batch.rows.get(&target.request.record))
                .and_then(|row| row["numeric_values"][&target.field_key].as_str());
        }
        values
    }

    pub(super) fn resolve(&self, feeds: &NumericFeeds, entity: Entity) -> NumericValues {
        let admitted = self
            .bindings
            .iter()
            .filter(|binding| !feeds.limited.contains(&(entity, binding.slot)))
            .cloned()
            .collect::<Vec<_>>();
        NumericValues::resolve(&admitted, |source| {
            let target = self
                .bindings
                .iter()
                .zip(&self.targets)
                .find(|(binding, _)| &binding.source == source)?
                .1
                .as_ref()?;
            feeds
                .batches
                .get(&target.group)?
                .rows
                .get(&target.request.record)?["numeric_values"][&target.field_key]
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_default()
    }
}

#[derive(Default)]
pub(super) struct Preparation {
    pub(super) inputs: HashMap<Entity, Input>,
    pub(super) demands: Vec<(Entity, bool)>,
    pub(super) inactive: Vec<Entity>,
    active: HashSet<Entity>,
}

impl Preparation {
    pub(super) fn refresh(
        &mut self,
        world: &World,
        previews: &mut QueryState<(Entity, &'static shader::EffectPreview, &'static Context)>,
    ) {
        let _preparation = nucleus::performance::Span::new("shader-numeric-input-preparation");
        self.demands.clear();
        self.inactive.clear();
        self.active.clear();
        let off = disabled(world);
        for (entity, preview, context) in previews.iter(world) {
            let bindings =
                (!preview.disabled && !off && !crate::native::laboratory::blocked(world, entity))
                    .then(|| shader::working(world, entity))
                    .flatten()
                    .map(|effect| effect.bindings.as_slice())
                    .filter(|bindings| !bindings.is_empty());
            let ready = bindings.is_some_and(|bindings| {
                if self
                    .inputs
                    .get(&entity)
                    .is_some_and(|input| input.matches(context, bindings))
                {
                    return true;
                }
                let Some(input) = Input::new(context, bindings) else {
                    return false;
                };
                self.inputs.insert(entity, input);
                true
            });
            if ready {
                self.active.insert(entity);
                self.demands.push((entity, preview.priority));
            } else {
                self.inactive.push(entity);
            }
        }
        self.inputs.retain(|entity, _| self.active.contains(entity));
        self.demands
            .sort_by_key(|(entity, priority)| (!*priority, entity.to_bits()));
        if self.demands.is_empty() {
            self.inputs = HashMap::new();
            self.active = HashSet::new();
            self.demands = Vec::new();
        }
        if self.inactive.is_empty() {
            self.inactive = Vec::new();
        }
    }

    pub(super) fn admit(&self) -> (BTreeMap<&str, Desired<'_>>, BTreeSet<(Entity, u8)>) {
        let _admission = nucleus::performance::Span::new("shader-numeric-demand-admission");
        let mut desired: BTreeMap<&str, Desired<'_>> = BTreeMap::new();
        let mut targets = BTreeSet::new();
        let mut limited = BTreeSet::new();
        for (entity, _) in &self.demands {
            let input = &self.inputs[entity];
            for (binding, target) in input.bindings.iter().zip(&input.targets) {
                let Some(target) = target else {
                    continue;
                };
                let group = target.group.as_str();
                if !desired.contains_key(group) && desired.len() >= 16 {
                    limited.insert((*entity, binding.slot));
                    continue;
                }
                let target_key = (group, target.request.record.as_str());
                if !targets.contains(&target_key) && targets.len() >= 256 {
                    limited.insert((*entity, binding.slot));
                    continue;
                }
                let batch = desired.entry(group).or_insert_with(|| Desired {
                    source: &target.source,
                    requests: BTreeMap::new(),
                });
                if batch.requests.len() >= 128 && !batch.requests.contains_key(target.key.as_str())
                {
                    limited.insert((*entity, binding.slot));
                    continue;
                }
                targets.insert(target_key);
                batch.requests.insert(target.key.as_str(), &target.request);
            }
        }
        (desired, limited)
    }
}

pub(super) struct Desired<'a> {
    pub(super) source: &'a Source,
    pub(super) requests: BTreeMap<&'a str, &'a protein::shader_inputs::Request>,
}

impl Desired<'_> {
    pub(super) fn matches(
        &self,
        requests: &BTreeMap<String, protein::shader_inputs::Request>,
    ) -> bool {
        self.requests.len() == requests.len()
            && self
                .requests
                .iter()
                .all(|(key, value)| requests.get(*key) == Some(*value))
    }
}
