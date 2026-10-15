use std::{cell::RefCell, path::Path};

type Hook = Box<dyn Fn(&Path, &str) -> crate::Result<()>>;

thread_local! {
    static HOOK: RefCell<Option<Hook>> = RefCell::new(None);
}

pub fn boundary(path: &Path, operation: &str) -> crate::Result<()> {
    HOOK.with(|hook| match hook.borrow().as_ref() {
        Some(hook) => hook(path, operation),
        None => Ok(()),
    })
}

pub struct Guard;

pub fn install(hook: impl Fn(&Path, &str) -> crate::Result<()> + 'static) -> Guard {
    HOOK.with(|current| {
        assert!(current.borrow().is_none());
        *current.borrow_mut() = Some(Box::new(hook));
    });
    Guard
}

impl Drop for Guard {
    fn drop(&mut self) {
        HOOK.with(|hook| {
            hook.borrow_mut().take();
        });
    }
}

#[cfg(test)]
mod tests;
