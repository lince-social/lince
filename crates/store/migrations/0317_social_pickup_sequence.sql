ALTER TABLE social_mailbox_pin ADD COLUMN pickup_sequence INTEGER NOT NULL DEFAULT 0;
DROP TABLE social_access_nonce;
