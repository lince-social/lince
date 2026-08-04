-- Device enrolment (Ontology §11 "profile vs device").
--
-- Enrolling a new device is pairing with YOURSELF, and it deserves its own
-- flow rather than reusing contact pairing: this token grants MEMBERSHIP IN
-- YOUR IDENTITY, which is strictly more than a contact QR grants. So it is
-- single-use and short-lived, and both properties are enforced here rather
-- than by the caller remembering.
--
-- Only the HASH is stored. The plaintext exists once, on the screen of the
-- Cell that issued it, and is never written to any row — a token sitting in
-- the database would be a second, quieter way into the identity.
CREATE TABLE enrolment_token (
    token_hash TEXT PRIMARY KEY,
    expires_at TEXT NOT NULL,
    used_at    TEXT,
    created_at TEXT NOT NULL
);
