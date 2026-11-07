use bevy::log::{
    tracing::{self, Subscriber, field::Visit, span::Attributes},
    tracing_subscriber::{self, layer::Context, prelude::*, registry::LookupSpan},
};
use lince_interface::host::performance;
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Name(String);

impl Visit for Name {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "name" {
            self.0 = value.to_owned();
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "name" {
            self.0 = format!("{value:?}").trim_matches('"').to_owned();
        }
    }
}

struct Body(Arc<str>);

struct Entered {
    id: tracing::span::Id,
    name: Arc<str>,
    wall: Instant,
    cpu: rustix::time::Timespec,
    children_cpu: Duration,
}

thread_local! {
    static ENTERED: RefCell<Vec<Entered>> = const { RefCell::new(Vec::new()) };
    static OVERFLOW: Cell<usize> = const { Cell::new(0) };
}

struct Systems;

fn cpu_delta(after: rustix::time::Timespec, before: rustix::time::Timespec) -> Option<Duration> {
    let delta = (i128::from(after.tv_sec) - i128::from(before.tv_sec)) * 1_000_000_000
        + i128::from(after.tv_nsec)
        - i128::from(before.tv_nsec);
    u64::try_from(delta).ok().map(Duration::from_nanos)
}

impl<S: Subscriber + for<'lookup> LookupSpan<'lookup>> tracing_subscriber::Layer<S> for Systems {
    fn on_new_span(
        &self,
        attributes: &Attributes<'_>,
        id: &tracing::span::Id,
        context: Context<'_, S>,
    ) {
        if attributes.metadata().name() != "system" {
            return;
        }
        let mut name = Name::default();
        attributes.record(&mut name);
        if (name.0.starts_with("bevy_") || name.0.starts_with("lince_interface::"))
            && name.0.len() <= 512
            && let Some(span) = context.span(id)
        {
            span.extensions_mut().insert(Body(name.0.into()));
        }
    }

    fn on_enter(&self, id: &tracing::span::Id, context: Context<'_, S>) {
        let Some(span) = context.span(id) else {
            return;
        };
        let extensions = span.extensions();
        let Some(body) = extensions.get::<Body>() else {
            return;
        };
        ENTERED.with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack.len() >= 128 || OVERFLOW.get() > 0 {
                OVERFLOW.set(OVERFLOW.get().saturating_add(1));
                performance::count(performance::Count::TimingCapacity);
                return;
            }
            stack.push(Entered {
                id: id.clone(),
                name: body.0.clone(),
                wall: Instant::now(),
                cpu: rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime),
                children_cpu: Duration::ZERO,
            });
        });
    }

    fn on_exit(&self, id: &tracing::span::Id, context: Context<'_, S>) {
        let Some(span) = context.span(id) else {
            return;
        };
        if span.extensions().get::<Body>().is_none() {
            return;
        }
        if OVERFLOW.get() > 0 {
            OVERFLOW.set(OVERFLOW.get() - 1);
            return;
        }
        let after = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
        ENTERED.with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack.last().is_none_or(|entry| entry.id != *id) {
                return;
            }
            let entry = stack.pop().unwrap();
            let Some(cpu) = cpu_delta(after, entry.cpu) else {
                performance::count(performance::Count::TimingRejected);
                return;
            };
            let Some(exclusive) = cpu.checked_sub(entry.children_cpu) else {
                performance::count(performance::Count::TimingRejected);
                return;
            };
            if let Some(parent) = stack.last_mut() {
                parent.children_cpu = parent.children_cpu.saturating_add(cpu);
            }
            performance::record_system(&entry.name, entry.wall.elapsed(), cpu, exclusive);
        });
    }
}

struct Events(tracing::Dispatch);

impl<S: Subscriber> tracing_subscriber::Layer<S> for Events {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        if self.0.enabled(event.metadata()) {
            self.0.event(event);
        }
    }
}

pub(super) fn enter() -> tracing::subscriber::DefaultGuard {
    let previous = tracing::dispatcher::get_default(Clone::clone);
    tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(Systems)
            .with(Events(previous)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_accounting_rejects_reversed_clocks_and_supports_second_boundaries() {
        let before = rustix::time::Timespec {
            tv_sec: 2,
            tv_nsec: 999_999_000,
        };
        let after = rustix::time::Timespec {
            tv_sec: 3,
            tv_nsec: 2_000,
        };
        assert_eq!(cpu_delta(after, before), Some(Duration::from_micros(3)));
        assert!(cpu_delta(before, after).is_none());
    }

    #[test]
    fn nested_and_reentered_system_spans_retire_on_the_executing_thread() {
        let _observer = enter();
        let parent = tracing::info_span!("system", name = "lince_interface::native::parent");
        let child = tracing::info_span!("system", name = "bevy_ui::layout::child");
        for _ in 0..2 {
            let _parent = parent.enter();
            {
                let _child = child.enter();
                ENTERED.with(|stack| assert_eq!(stack.borrow().len(), 2));
            }
            ENTERED.with(|stack| assert_eq!(stack.borrow().len(), 1));
        }
        ENTERED.with(|stack| assert!(stack.borrow().is_empty()));
    }
}
