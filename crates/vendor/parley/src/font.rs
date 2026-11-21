// Copyright 2021 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// non-lean-yet
// Scope: struct FontContext.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use fontique::Collection;

use fontique::SourceCache;

/// A font database/cache (wrapper around a Fontique [`Collection`] and [`SourceCache`]).
///
/// This type is designed to be a global resource with only one per-application (or per-thread).
#[derive(Default, Clone)]
pub struct FontContext {
    pub collection: Collection,
    pub source_cache: SourceCache,
}

impl FontContext {
    /// Create a new `FontContext`, discovering system fonts if available.
    pub fn new() -> Self {
        Self::default()
    }
}
