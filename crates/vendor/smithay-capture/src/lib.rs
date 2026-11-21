// non-lean-yet
// Scope: mod wayland.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 3,
//   5, 6.

pub use smithay::{backend, output, reexports, utils};

pub mod wayland {
    pub use smithay::wayland::{dmabuf, foreign_toplevel_list, shm};
    pub mod image_capture_source;
    pub mod image_copy_capture;
}
