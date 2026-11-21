// non-lean-yet
// Scope: struct Values, struct Current, struct Prepared.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 64, 67, 69, 82.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (81).

use super::*;
use nucleus::shader_effect::{ResolvedInputs, bindings::NumericValues, gpu_quantity};

#[derive(Clone, Copy)]
pub(super) struct Values {
    pub quantity: Option<f32>,
    pub mapped: ResolvedInputs,
    pub values: [f32; 8],
    pub present: [u32; 8],
}

struct Current {
    definition: Arc<Effect>,
    quantity: Option<String>,
    numeric: NumericValues,
    bindings_match: bool,
    program: Arc<str>,
    values: Values,
}

#[derive(Default)]
pub(super) struct Prepared(Option<Current>);

impl Prepared {
    pub(super) fn mapped(
        &self,
        definition: &Arc<Effect>,
        quantity: Option<&str>,
    ) -> ResolvedInputs {
        self.0
            .as_ref()
            .filter(|current| {
                Arc::ptr_eq(&current.definition, definition)
                    && current.quantity.as_deref() == quantity
            })
            .map_or_else(
                || definition.inputs.resolve(quantity),
                |current| current.values.mapped,
            )
    }

    pub(super) fn resolve(
        &mut self,
        definition: &Arc<Effect>,
        quantity: Option<&str>,
        numeric: &NumericValues,
        bindings_match: bool,
    ) -> (Arc<str>, Values) {
        let _preparation = nucleus::performance::Span::new("shader-frame-readings");
        let same = self.0.as_ref().is_some_and(|current| {
            Arc::ptr_eq(&current.definition, definition)
                && current.quantity.as_deref() == quantity
                && current.bindings_match == bindings_match
                && (!bindings_match || &current.numeric == numeric)
        });
        if !same {
            let empty = NumericValues::default();
            let numeric = if bindings_match { numeric } else { &empty };
            let program = definition.selected_with_inputs(quantity, numeric);
            let program = self
                .0
                .as_ref()
                .filter(|current| current.program.as_ref() == program)
                .map_or_else(|| Arc::from(program), |current| current.program.clone());
            let (values, present) = numeric.gpu();
            self.0 = Some(Current {
                definition: definition.clone(),
                quantity: quantity.map(str::to_owned),
                numeric: numeric.clone(),
                bindings_match,
                program,
                values: Values {
                    quantity: gpu_quantity(quantity),
                    mapped: definition.inputs.resolve(quantity),
                    values,
                    present,
                },
            });
            nucleus::performance::add("shader-frame-readings-resolved", 1);
        }
        let current = self.0.as_ref().unwrap();
        (current.program.clone(), current.values)
    }
}
