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
