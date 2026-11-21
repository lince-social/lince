// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// Trait for types that represent the color of glyphs or decorations.
// non-lean-yet
// Scope: trait Brush.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

pub trait Brush: Clone + PartialEq + Default + core::fmt::Debug {}

impl<T: Clone + PartialEq + Default + core::fmt::Debug> Brush for T {}
