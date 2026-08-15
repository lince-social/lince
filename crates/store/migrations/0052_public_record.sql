-- The signed PUBLIC directory record (Ontology §11, cluster C3).
--
-- The bytes are a complete pkarr `SignedPacket`: public key, signature,
-- timestamp and the DNS packet naming this Organ's front-door Cells. Stored
-- rather than re-signed on every republish, because a DHT entry expires in
-- hours and re-signing would require the ROOT key online forever — undoing the
-- offline-root split the identity floor is built on. A front-door Cell holding
-- no signing material can broadcast these bytes verbatim.
--
-- Local-only, never synced: it is derived from the roster, and every contact
-- who can read it can also resolve it from the network by the same key.
CREATE TABLE organ_public_record (
    organ_uid  TEXT PRIMARY KEY,
    packet     BLOB NOT NULL,
    updated_at TEXT NOT NULL
);
