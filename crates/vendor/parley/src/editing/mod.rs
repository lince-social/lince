// Copyright 2025 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// non-lean-yet
// Scope: mod cursor, mod editor, mod selection.
// BeautifulRust (baseline lines):
// - [review: §9 Modules] Check the intended public API and re-export individual items where
//   possible. Sites 8, 9, 10.

mod cursor;
mod editor;
mod selection;

pub use self::cursor::*;
pub use self::editor::*;
pub use self::selection::*;
