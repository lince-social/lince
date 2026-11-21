// non-lean-yet
// Scope: struct NearbyPeer.
// BeautifulRust (baseline lines):
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 2, 3,
//   4, 5.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyPeer {
    pub node_id: String,
    pub fingerprint: String,
    pub name: String,
}
