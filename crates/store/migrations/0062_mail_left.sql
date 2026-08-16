-- What THIS Cell left with a carrier, so a notice about it can be believed.
--
-- The carrier tells a sender when their bundle expired uncollected. Without a
-- local record that claim is unverifiable: any box a Cell ever dialed could
-- announce that mail nobody sent was never picked up, and the surface would
-- dutifully alarm somebody about a message they never wrote. So a notice is
-- only ever shown when it names a uid THIS Cell recorded here, and a uid with
-- no row is dropped in silence.
--
-- It never syncs. Where a Cell leaves its mail is the same correspondence
-- pattern the carrier's own tables refuse to share; handing it to every device
-- of every contact would be worse, not better.
CREATE TABLE mail_left (
    -- The carrier's id for the bundle, which is what comes back in a notice.
    uid TEXT PRIMARY KEY,
    -- Whose box it went to, and where. The node id is how the sender finds
    -- the carrier again to ask; the Organ uid is what a person is shown.
    carrier_organ TEXT NOT NULL,
    carrier_node TEXT NOT NULL,
    -- Who the mail was FOR. The one fact that makes the notice worth showing:
    -- "your message to Ana was never picked up" is actionable, "a bundle
    -- expired" is noise.
    to_organ TEXT NOT NULL,
    left_at TEXT NOT NULL,
    -- Set when the carrier reported this bundle expired uncollected. Kept
    -- rather than deleted, because the point of the whole box is that a person
    -- gets told, and a row deleted on arrival would be a notice nobody saw.
    expired_at TEXT
);

CREATE INDEX mail_left_by_carrier ON mail_left (carrier_node, expired_at);
CREATE INDEX mail_left_by_recipient ON mail_left (to_organ, expired_at);
