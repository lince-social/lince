// non-lean-yet
// Scope: struct Page, fn foundations, fn pages, fn reading.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 2, 3,
//   4, 5, 8, 16, 19.

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Page {
    pub slug: &'static str,
    pub chapter: &'static str,
    pub reference: Option<&'static str>,
}

pub fn foundations(subject: &str) -> &'static [&'static str] {
    crate::content::get()
        .foundations
        .get(subject)
        .copied()
        .unwrap_or(&[])
}

pub fn pages() -> &'static [Page] {
    &crate::content::get().pages
}
pub fn reading() -> &'static [crate::practice::Lesson] {
    &crate::content::get().reading
}
