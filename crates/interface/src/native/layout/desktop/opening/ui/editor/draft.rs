// non-lean-yet
// Scope: struct RuleSource, struct Source, fn fields, struct Draft.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 54.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   78.

use super::*;
use crate::native::layout::desktop::input_settings::prepared::Prepared;

struct RuleSource {
    enabled: bool,
    all: bool,
    properties: Vec<Property>,
}

struct Source {
    revision: u64,
    creating: bool,
    workspace: Option<u64>,
    output: Option<String>,
    flags: [Option<bool>; 3],
    rule: Option<RuleSource>,
}

impl Source {
    fn matches(&self, state: &EditorState) -> bool {
        self.revision == state.revision
            && self.creating == state.creating
            && self.workspace == state.form.workspace
            && self.output == state.form.output
            && self.flags == state.form.flags
            && match (&self.rule, &state.kind) {
                (None, Kind::Window) => true,
                (
                    Some(previous),
                    Kind::Rule {
                        enabled,
                        all,
                        conditions,
                        ..
                    },
                ) => {
                    previous.enabled == *enabled
                        && previous.all == *all
                        && previous
                            .properties
                            .iter()
                            .copied()
                            .eq(conditions.iter().map(|condition| condition.property))
                }
                _ => false,
            }
    }

    fn capture(state: &EditorState) -> Self {
        Self {
            revision: state.revision,
            creating: state.creating,
            workspace: state.form.workspace,
            output: state.form.output.clone(),
            flags: state.form.flags,
            rule: match &state.kind {
                Kind::Window => None,
                Kind::Rule {
                    enabled,
                    all,
                    conditions,
                    ..
                } => Some(RuleSource {
                    enabled: *enabled,
                    all: *all,
                    properties: conditions
                        .iter()
                        .map(|condition| condition.property)
                        .collect(),
                }),
            },
        }
    }
}

fn fields(state: &EditorState) -> impl Iterator<Item = Entity> + Clone + '_ {
    let conditions = match &state.kind {
        Kind::Window => &[][..],
        Kind::Rule { conditions, .. } => conditions.as_slice(),
    };
    state
        .form
        .inputs()
        .into_iter()
        .chain(conditions.iter().map(|condition| condition.value))
        .chain(state.name.filter(|_| state.creating))
}

#[derive(Default)]
pub(in crate::native::layout::desktop::opening::ui) struct Draft {
    fields: Prepared<Source, Payload>,
}

impl Draft {
    pub(super) fn get(
        &self,
        world: &World,
        state: &EditorState,
    ) -> Option<&Result<Payload, String>> {
        self.fields
            .get_matching(world, fields(state), |source| source.matches(state))
    }

    pub(super) fn prepare(
        &mut self,
        world: &World,
        state: &EditorState,
    ) -> &Result<Payload, String> {
        let _preparation = engine::performance::Span::new("opening-editor-draft");
        self.fields.prepare_matching(
            world,
            fields(state),
            |source| source.matches(state),
            || Source::capture(state),
            || parse_candidate(world, state),
        )
    }
}
