-- Ontology, cluster 4: the offer lifecycle's missing half — a refusal you can
-- remember without the sender learning you did.
--
-- Four handshakes spell one conversation (an offer lands, it waits, you accept
-- or refuse), and three of them could not remember a refusal at all. The thread
-- invite's forgetting was defended as a privacy decision: a sender must not be
-- able to tell "declined" from "ignored", because that difference is a probe.
--
-- That defence conflates two different questions, and separating them is what
-- this table is for:
--
--   WHAT THE SENDER LEARNS must stay identical for declined and ignored. That
--   is a real rule and nothing here weakens it — every path that answers a
--   sender still answers exactly what it answered before.
--
--   WHAT I REMEMBER LOCALLY is nobody's business but mine. Destroying my own
--   memory to protect the sender's ignorance protected nothing and cost
--   something: a stranger whose invite I declined could invite me again
--   immediately, and again, and I would be prompted every time. The anti-spam
--   hole was wearing the privacy rule as a disguise.
--
-- So: local only, never synced, never logged as an op — the same treatment
-- `thread_invite` itself gets and for the same reason.
--
-- `until` is when the refusal stops suppressing a fresh offer. A permanent
-- block would be wrong: people change their minds and circumstances change,
-- and a refusal that never expires turns one bad moment into a life sentence.
CREATE TABLE offer_refusal (
    kind        TEXT NOT NULL,
    subject_uid TEXT NOT NULL,
    other_party TEXT NOT NULL,
    at          TEXT NOT NULL,
    until       TEXT NOT NULL,
    PRIMARY KEY (kind, subject_uid, other_party)
);

CREATE INDEX offer_refusal_by_party ON offer_refusal (other_party, kind);
