UPDATE transfer_party
SET party_uid = 'party-' || transfer_id || '-' || id
WHERE party_uid IS NULL;

UPDATE transfer_structured_item
SET item_uid = 'item-' || transfer_id || '-' || id
WHERE item_uid IS NULL;

UPDATE transfer_interaction
SET interaction_uid = 'interaction-' || transfer_id || '-' || id
WHERE interaction_uid IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_party_uid
ON transfer_party(transfer_id, party_uid);

CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_structured_item_uid
ON transfer_structured_item(transfer_id, item_uid);

CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_interaction_uid
ON transfer_interaction(transfer_id, interaction_uid);
