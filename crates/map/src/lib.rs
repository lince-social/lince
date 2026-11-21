// non-lean-yet
// Scope: mod diagnostics, mod geometry, mod navigation, mod pack.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 2,
//   3, 4, 5, 6, 7.

pub mod diagnostics;
pub mod geometry;
pub mod navigation;
pub mod pack;
pub mod region;
pub mod scene;
pub mod source;

pub use galileo;
pub use geometry::{Bounds, Camera, Coordinate, Tile};
pub use source::{Progress, Settings, TileStore};
