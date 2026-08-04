//! Organs seen on the local network.
//!
//! This lives in the nucleus rather than in the engine because it is read one
//! layer BELOW where it is produced: the wire fills the list, and Protein
//! serves it. Protein cannot depend on the engine, so the shape they agree on
//! has to sit under both.
//!
//! Nothing here is persisted, and that is deliberate — discovery results are
//! transient, and mirroring them anywhere durable would record who is on your
//! local network. The list is the presence: a peer that stops announcing is
//! removed, so there is no "last seen" to carry.

/// One Organ announcing itself on this LAN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyPeer {
    pub node_id: String,
    /// Short prefix of the NodeId, for telling many rows apart in a list.
    /// DISAMBIGUATION, never a security check: under iroh the address already
    /// is the key, so there is nothing here left to verify.
    pub fingerprint: String,
    /// The peer's self-declared label. UNTRUSTED: anyone can advertise any
    /// name, so a surface must render it as a claim and never as identity.
    /// Empty when the peer published none.
    pub name: String,
}
