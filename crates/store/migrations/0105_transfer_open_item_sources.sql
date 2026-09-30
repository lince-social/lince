CREATE TABLE transfer_open_claim_pair_next (
    uid                    TEXT PRIMARY KEY,
    transfer_uid           TEXT NOT NULL REFERENCES transfer(record_uid),
    source_promise_uid     TEXT NOT NULL REFERENCES promise(uid),
    source_record_uid      TEXT REFERENCES record(uid),
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

INSERT INTO transfer_open_claim_pair_next SELECT * FROM transfer_open_claim_pair;
DROP TABLE transfer_open_claim_pair;
ALTER TABLE transfer_open_claim_pair_next RENAME TO transfer_open_claim_pair;

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
      AND NEW.source_record_uid IS source.record_uid
      AND proposer.record_uid IS NEW.source_record_uid
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
