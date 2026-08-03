-- iroh peer addressing (Ontology §11 "Transport: iroh"). A contact is reached
-- by NodeId — under iroh the address IS the key, so dialing one reaches that
-- keypair or nothing. `base_url`/`last_seen_addr` stay as debugging
-- breadcrumbs; neither is a routing input any more.
--
-- Deliberately SEPARATE from `identity_key`: the node key authenticates a LIVE
-- CONNECTION and is per-Cell, cheap to rotate, and never signs durable bytes.
-- The identity key authenticates durable bytes and is per-Organ. Fusing them
-- would mean compromising any running Cell forges that Organ's history forever.
ALTER TABLE organ_contact ADD COLUMN node_id TEXT;

-- One NodeId belongs to one contact: the accept path resolves an inbound
-- connection's `remote_id()` through this index, so a duplicate would make
-- "who is on this socket" ambiguous — which is the one question the transport
-- layer exists to answer.
CREATE UNIQUE INDEX idx_organ_contact_node_id
    ON organ_contact(node_id) WHERE node_id IS NOT NULL;
