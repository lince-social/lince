pub mod no_comments;

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub complaint: String,
}

pub trait Rule: Send + Sync {
    fn name(&self) -> &'static str;

    fn wants(&self, path: &Path) -> bool {
        path.extension().and_then(|value| value.to_str()) == Some("rs")
    }

    fn inspect(&self, source: &str) -> Vec<Finding>;

    fn fix(&self, _source: &str) -> Option<String> {
        None
    }

    fn remedy(&self) -> &'static str;
}

pub fn enforced() -> Vec<Box<dyn Rule>> {
    vec![Box::new(no_comments::NoComments)]
}
