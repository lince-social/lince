use std::ops::ControlFlow;

use super::ComponentState;

impl ComponentState {
    /// Borrows the primary Record for a Record presentation.
    pub fn record(&self) -> Option<&str> {
        match self {
            Self::Record { record, .. } => Some(record),
            _ => None,
        }
    }

    /// Mutably borrows the primary Record for a Record presentation.
    pub fn record_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Record { record, .. } => Some(record),
            _ => None,
        }
    }

    /// Collects mutable Record references in the same order as [Self::records].
    pub fn records_mut(&mut self) -> Vec<&mut String> {
        let mut records = Vec::new();
        self.visit_records_mut(&mut |record| records.push(record));
        records
    }

    pub(crate) fn visit_records_mut<'a>(&'a mut self, visit: &mut impl FnMut(&'a mut String)) {
        match self {
            Self::Record {
                record, start_call, ..
            } => {
                visit(record);
                if let Some(start) = start_call {
                    visit(&mut start.thread);
                    visit(&mut start.person);
                }
            }
            Self::Composition { composition } => {
                for part in &mut composition.parts {
                    part.component.visit_records_mut(visit);
                }
            }
            _ => {}
        }
    }

    /// Collects Record references in depth-first part order.
    ///
    /// For a Record with an automatic call, the Record comes before its thread and Person.
    pub fn records(&self) -> Vec<&str> {
        let mut records = Vec::new();
        let _: ControlFlow<()> = self.visit_records(&mut |record| {
            records.push(record);
            ControlFlow::Continue(())
        });
        records
    }

    /// Visits each borrowed Record reference in [Self::records] order and propagates an early exit.
    pub fn visit_records<'a, B>(
        &'a self,
        visit: &mut impl FnMut(&'a str) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        match self {
            Self::Record {
                record, start_call, ..
            } => {
                visit(record)?;
                if let Some(start) = start_call {
                    visit(&start.thread)?;
                    visit(&start.person)?;
                }
            }
            Self::Composition { composition } => {
                for part in &composition.parts {
                    part.component.visit_records(visit)?;
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}
