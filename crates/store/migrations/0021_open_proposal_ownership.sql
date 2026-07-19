-- OPEN means an unnamed counterparty, not an unnamed owner. Legacy rows lacked
-- proposer evidence, so only an unambiguous Transfer creator may backfill it.
CREATE TABLE transfer_open_owner_backfill_guard (
    valid INTEGER NOT NULL CHECK (valid = 1)
) STRICT;

INSERT INTO transfer_open_owner_backfill_guard(valid)
SELECT 0
WHERE EXISTS (
    SELECT 1
    FROM promise open_promise
    WHERE open_promise.state = 'open'
      AND open_promise.party_uid IS NULL
      AND (
          SELECT COUNT(*) FROM transfer_party creator
          WHERE creator.transfer_uid = open_promise.transfer_uid
            AND creator.kind = 'creator'
      ) != 1
);

UPDATE promise
SET party_uid = (
    SELECT creator.actor_uid
    FROM transfer_party creator
    WHERE creator.transfer_uid = promise.transfer_uid
      AND creator.kind = 'creator'
)
WHERE state = 'open' AND party_uid IS NULL;

DROP TABLE transfer_open_owner_backfill_guard;

CREATE TRIGGER transfer_open_promise_requires_proposer_insert
BEFORE INSERT ON promise
WHEN NEW.state = 'open' AND NEW.party_uid IS NULL
BEGIN
    SELECT RAISE(ABORT, 'OPEN promise requires a proposer Person');
END;

CREATE TRIGGER transfer_open_promise_requires_proposer_update
BEFORE UPDATE OF state, party_uid ON promise
WHEN NEW.state = 'open' AND NEW.party_uid IS NULL
BEGIN
    SELECT RAISE(ABORT, 'OPEN promise requires a proposer Person');
END;

CREATE TRIGGER transfer_open_promise_requires_party_insert
BEFORE INSERT ON promise
WHEN NEW.state = 'open' AND NEW.transfer_uid IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM transfer_party party
    WHERE party.transfer_uid = NEW.transfer_uid
      AND party.actor_uid = NEW.party_uid
)
BEGIN
    SELECT RAISE(ABORT, 'OPEN proposer must be a current Transfer party');
END;

CREATE TRIGGER transfer_open_promise_requires_party_update
BEFORE UPDATE OF state, party_uid, transfer_uid ON promise
WHEN NEW.state = 'open' AND NEW.transfer_uid IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM transfer_party party
    WHERE party.transfer_uid = NEW.transfer_uid
      AND party.actor_uid = NEW.party_uid
)
BEGIN
    SELECT RAISE(ABORT, 'OPEN proposer must be a current Transfer party');
END;

CREATE TABLE transfer_open_claim_pair (
    uid                    TEXT PRIMARY KEY,
    transfer_uid           TEXT NOT NULL REFERENCES transfer(record_uid),
    source_promise_uid     TEXT NOT NULL REFERENCES promise(uid),
    source_record_uid      TEXT NOT NULL REFERENCES record(uid),
    proposer_promise_uid   TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    claimant_promise_uid   TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    proposer_person_uid    TEXT NOT NULL REFERENCES record(uid),
    claimant_person_uid    TEXT NOT NULL REFERENCES record(uid),
    revision               INTEGER NOT NULL CHECK (revision > 0),
    reuse_policy           TEXT NOT NULL CHECK (reuse_policy IN ('duplicate', 'consume')),
    idempotency_key        TEXT NOT NULL UNIQUE,
    created_at             TEXT NOT NULL,
    CHECK (proposer_person_uid != claimant_person_uid),
    CHECK (proposer_promise_uid != claimant_promise_uid),
    FOREIGN KEY (transfer_uid, revision)
        REFERENCES transfer_revision(transfer_uid, revision),
    FOREIGN KEY (idempotency_key)
        REFERENCES transfer_revision(idempotency_key)
) STRICT;

CREATE INDEX transfer_open_claim_pair_source
    ON transfer_open_claim_pair(source_promise_uid, revision, uid);

CREATE TRIGGER transfer_open_claim_pair_matches_promises
BEFORE INSERT ON transfer_open_claim_pair
WHEN NOT EXISTS (
    SELECT 1
    FROM promise proposer
    JOIN promise claimant ON claimant.uid = NEW.claimant_promise_uid
    JOIN promise source ON source.uid = NEW.source_promise_uid
    WHERE proposer.uid = NEW.proposer_promise_uid
      AND proposer.transfer_uid = NEW.transfer_uid
      AND claimant.transfer_uid = NEW.transfer_uid
      AND proposer.party_uid = NEW.proposer_person_uid
      AND source.party_uid = NEW.proposer_person_uid
      AND NEW.source_record_uid = source.record_uid
      AND proposer.record_uid = NEW.source_record_uid
      AND claimant.party_uid = NEW.claimant_person_uid
      AND proposer.state = 'proposed'
      AND claimant.state = 'proposed'
      AND proposer.revision = NEW.revision
      AND claimant.revision = NEW.revision
      AND proposer.delta = -claimant.delta
      AND proposer.concept_uid IS claimant.concept_uid
      AND proposer.unit_uid IS claimant.unit_uid
      AND proposer.window_start IS claimant.window_start
      AND proposer.window_end IS claimant.window_end
      AND proposer.location_lat IS claimant.location_lat
      AND proposer.location_lon IS claimant.location_lon
      AND proposer.location_address IS claimant.location_address
      AND proposer.condition IS claimant.condition
      AND proposer.reserve_from = claimant.reserve_from
      AND (
          (NEW.reuse_policy = 'consume'
           AND proposer.uid = NEW.source_promise_uid
           AND source.state = 'proposed'
           AND claimant.source_promise_uid = NEW.source_promise_uid)
          OR
          (NEW.reuse_policy = 'duplicate'
           AND proposer.source_promise_uid = NEW.source_promise_uid
           AND source.state = 'open'
           AND claimant.source_promise_uid = NEW.source_promise_uid)
      )
)
BEGIN
    SELECT RAISE(ABORT, 'OPEN claim pair does not match its concrete promises');
END;

CREATE TRIGGER transfer_open_claim_pair_immutable_update
BEFORE UPDATE ON transfer_open_claim_pair
BEGIN
    SELECT RAISE(ABORT, 'OPEN claim pairs are immutable');
END;

CREATE TRIGGER transfer_open_claim_pair_immutable_delete
BEFORE DELETE ON transfer_open_claim_pair
BEGIN
    SELECT RAISE(ABORT, 'OPEN claim pairs are immutable');
END;
