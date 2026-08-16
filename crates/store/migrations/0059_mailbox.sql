-- The blind mailbox: store-and-forward for OTHER Organs (Ontology C4).
--
-- A mailbox holds SEALED bundles it cannot read and hands them over when the
-- recipient appears. It is not a member of any roster, holds no key that opens
-- anything it carries, and CONVERGES NOTHING — it stores and forwards.
--
-- That last property is why these tables stand apart from everything else in
-- the schema. There is no path from here into `sync_op` or the read model: a
-- bundle is an opaque blob with routing metadata around it, and nothing in the
-- engine turns one into an op except a recipient Cell that could open it,
-- which by construction the carrier is not. Following 0055's argument — a
-- property you can defeat by pointing a second client at the store is not a
-- security property — the isolation is structural rather than a promise the
-- callers keep.
--
-- Neither table syncs. A carrier's ledger of who left what for whom is the
-- most sensitive thing it holds, and it is nobody's business but the
-- operator's; putting it on a synced table would hand every contact the
-- correspondence pattern of every registered recipient.

-- Who this box carries mail FOR. A recipient registers; a sender never does.
--
-- Registration is what replaces routing. A pool of mailboxes behind a load
-- balancer cannot work — route randomly and the recipient must poll every box,
-- keep a routing table and the pool is one logical mailbox with one operator —
-- so a bundle goes to a box the RECIPIENT named, and that box knows the name.
CREATE TABLE mailbox_registration (
    organ_uid TEXT PRIMARY KEY,
    -- The recipient's ROOT key as it was at registration.
    --
    -- Stored instead of a snapshot of their device addresses, because the
    -- addresses are the thing that changes: a recipient who enrols a new phone
    -- must be able to collect from it. At collection they present their
    -- current signed roster, which has to chain from this key, and the
    -- connection must be one of the Cells that roster names. So the mailbox
    -- tracks device changes without ever polling for them, and a stolen device
    -- stops collecting as soon as its Organ revokes it.
    root_key TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    -- What this recipient may consume, in bytes. A deposit is charged to the
    -- RECIPIENT, never the sender: abuse then costs the person who chose this
    -- mailbox rather than its operator, which is the right incentive and lets
    -- an operator size the disk by member count.
    quota_bytes INTEGER NOT NULL,
    registered_at TEXT NOT NULL
);

-- The sealed bundles themselves.
CREATE TABLE mailbox_bundle (
    uid TEXT PRIMARY KEY,
    to_organ TEXT NOT NULL REFERENCES mailbox_registration(organ_uid) ON DELETE CASCADE,
    -- Unsealed routing metadata, and deliberately the ONLY thing the carrier
    -- learns. It knows who wrote to whom, when, and how much, and nothing
    -- about what was said. That cost is stated in the interface rather than
    -- buried in a README, because with a mutual friend as carrier it is
    -- socially real — and it is exactly why the RECIPIENT chooses the mailbox.
    from_organ TEXT NOT NULL,
    from_cell TEXT NOT NULL,
    -- The node id the deposit ARRIVED on, which is the only sender fact here
    -- that is proven rather than claimed: the two above are read out of a
    -- bundle a stranger wrote, while this one is the QUIC peer identity the
    -- transport verified. Everything told back to a sender later — the expiry
    -- notice — is scoped by this column and never by the other two.
    from_node TEXT NOT NULL DEFAULT '',
    -- The `SealedBundle` as JSON. Opaque here. Nothing in this database ever
    -- parses it beyond the routing fields already lifted out above.
    body TEXT NOT NULL,
    bytes INTEGER NOT NULL,
    received_at TEXT NOT NULL,
    -- Held this long, then deleted. The same number as the sealing-key
    -- retention window (`engine::seal::RETENTION_DAYS`) — old private keys
    -- survive the window plus grace, so nothing uncollected is ever lost to a
    -- rotation, and anything past it is unopenable by everyone.
    expires_at TEXT NOT NULL
);

CREATE INDEX mailbox_bundle_by_recipient ON mailbox_bundle (to_organ, received_at);
CREATE INDEX mailbox_bundle_by_expiry ON mailbox_bundle (expires_at);

-- What a swept bundle leaves behind so its SENDER can be told.
--
-- The 30-day rule promises the sender learns their mail was never collected. A
-- plain DELETE forecloses that: once the row is gone there is nothing left to
-- say who to tell. So expiry moves the routing metadata here and drops the
-- ciphertext, which is the whole point — the carrier can report the failure
-- while holding nothing it could not already see.
--
-- A message that vanishes with nobody knowing is the one failure people trust
-- a messaging system not to have.
CREATE TABLE mailbox_expiry_notice (
    uid TEXT PRIMARY KEY,
    to_organ TEXT NOT NULL,
    from_organ TEXT NOT NULL,
    from_cell TEXT NOT NULL,
    -- Carried over from the bundle, and the only thing a sender is matched on
    -- when it comes back to ask what expired.
    from_node TEXT NOT NULL DEFAULT '',
    bytes INTEGER NOT NULL,
    received_at TEXT NOT NULL,
    expired_at TEXT NOT NULL,
    -- Stamped when the sender was handed this notice, and the row is DELETED
    -- when they acknowledge it — the same shape as a collected bundle. A row
    -- marked told and kept forever would leave the carrier holding the one
    -- thing this schema says is nobody's business but the operator's: a
    -- permanent ledger of who wrote to whom.
    notified_at TEXT
);

CREATE INDEX mailbox_expiry_notice_pending ON mailbox_expiry_notice (from_node, notified_at);
