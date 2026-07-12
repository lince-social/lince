-- Blueprint VIII.3: delivery/receipt confirmations. When a transfer demands
-- confirmation, settlement (active -> kept) is gated on two annotation facts
-- on the transfer record: one 'delivery' and one 'receipt'.
ALTER TABLE transfer ADD COLUMN require_confirmation INTEGER NOT NULL DEFAULT 0;
