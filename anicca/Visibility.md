## Visibility: what a logged-in Organ may see

When someone logs into your Organ and can read your records, they see
everything by default. You choose to hide things instead of choosing to
share them.

### Done

- **Hide a whole record from a contact.** In the contact panel, there is a
  "Never send" field where you list records that a specific contact should
  never receive. If a hidden record is linked to another record, that link
  is hidden too, so a hidden record's connections can't be seen either.
  Hiding only affects what's sent from now on — it doesn't take back
  anything they already have, and un-hiding doesn't resend anything they
  missed while it was hidden.

- **Choose which parts of a record a contact sees, not just yes/no.** A
  record can have several parts (e.g. its title vs. its full content). For
  each contact you can choose one of three settings: share everything, share
  only the parts you pick, or share nothing but the fact that the record
  exists. Some things always travel as a whole (you can't split a shared
  document into "just the title"); the choice tells you clearly when a
  setting isn't possible.

- **Deleted records are always reported as deleted**, even to a narrowed
  contact — otherwise their copy would never know the record was removed
  and would live on forever on their side.

- **Widening what a contact sees catches them up automatically.** If you add
  a part to what a contact can see, they get everything they missed for that
  part, not just new changes going forward. Narrowing never does this in
  reverse — it only stops future sharing, it never retracts what they
  already received.

- **If your sharing settings for a contact can't be read** (corrupted or
  broken), Lince treats that contact as fully unnarrowed for safety, and
  tells you clearly in the panel that the setting isn't being applied and
  shows you the broken value, so you can fix it by re-saving.

- **When you unhide a contact, they get caught up properly**, including
  anything that changed for that record while it was hidden — not just
  future changes.

- **The same "which parts of a record" picker is used everywhere**, not
  just here. It's the same one already used elsewhere to choose what a
  shared record includes, so learning it in one place carries over to the
  other.

### What is left

- **A summary view of what each contact can see of you.** Today you can set
  each contact's sharing level one at a time in that contact's own panel,
  but there's no single screen listing all contacts side by side with what
  each one currently sees. *Technical: `organ_contact.scope_fields` is set
  and read per-contact already; this is a read-only aggregate view over the
  existing `organ_contact` rows, no new storage or sync change.*

- **Per-device sharing limits.** Right now a device you've logged in from
  (a phone, another computer) either has full access to your Organ or is
  logged out entirely — there's no in-between where you let a device see
  less than everything. *Technical: device/session membership today is
  tracked as an all-or-nothing entry in the roster (`identity_key` /
  revocation), with no `scope_fields`-style column-narrowing on it; adding
  this means giving devices the same kind of scope the per-contact sharing
  narrowing already has, applied to roster membership instead of contacts.*
