//! Public discovery questions, requests, and response pages.

mod page;
mod question;

pub use page::{Header, Page, Request};
pub use question::Question;

pub const MAX_HEADERS: usize = 8;
pub const MAX_QUESTION_BYTES: usize = 12_288;
pub const MAX_CURSOR_BYTES: usize = 2_048;
pub const PAGE_WORK: usize = 32;

#[cfg(test)]
mod tests;
