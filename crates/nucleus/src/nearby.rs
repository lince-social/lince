#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyPeer {
    pub node_id: String,
    pub fingerprint: String,
    pub name: String,
}
