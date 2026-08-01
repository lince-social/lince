Storytelling: Your data is called a DNA, your individual Lince instance is called a Cell. When you have many users in a Cell we call it an Organ. But virtually other Lince apps in other computers are always called Organs. This Organ can be for your family, friends, company, party, whatever.

People can use it to share part of their Needs public in this Organ, and Contribute to the Needs of the Organ. Lince is all of the Cells and Organs are working together for each other's Needs. Your day-to-day using Lince is to mostly to manage your Cell.

- [ ] CRUD of Organs with levels or proximity (number) and trust (unkown, blocked, known) (Organs are a Kind of Record).
- [x] Sands can point to organs to get data from there, if you can login to a user in an organ you can take data from private ones.
- [ ] All Records are private by default, make a feature to show a Record to some Organs, filtered by trust and/or proximity.

- [x] **Organ** — Protein list of `kind=organ` records (this Cell + its
  contacts); selecting one shows/edits its `lince.file_sync` extension
  (enabled, disk path) via `set-extension` — File Sync to disk as a
  first-class per-organ feature. Deliberately thin: no trust/proximity/
  introduce/block/quarantine UI yet (that's the separate **Organ contacts
  manager** item below); the dormant, unregistered pre-Protein
  `organ_management` sand was left in place rather than adapted.
- [ ] **Organ contacts manager** — list contacts with trust/proximity/sync
  policy, introduce via URL, block button, quarantine viewer.
