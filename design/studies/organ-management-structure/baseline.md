# Rust capability baseline

The Castle currently has eight flat tabs: Local settings, Organs, Nearby, Add contact, My devices, Mail, Discovery and Public profile. It opens Organs. Local settings nests Configuration, owner backup and several sync workflows. Public discovery nests its own administration and recovery tools.

All alternatives share the page catalogue below. Prototype identifiers point to controls; source references ground native behavior. Prototype forms never dispatch native actions. Dynamic per-row controls use synthetic records. See `source-inventory.json` for individual source references.

| Focused page | Existing controls and behavior |
| --- | --- |
| Identity | Save identity |
| Organ Records | Save Organ; Delete Organ; Save Organ Record; Delete Organ Record; Reload selected Organ |
| Share my Organ | Copy pairing code |
| Contacts |  |
| Contact | Rename locally; Reconnect and refresh device list; Synchronize now; Start conversation; Forget contact; Open conversation; Open live Organ |
| Sharing & trust | Save trust; Save proximity; Save feed direction; Save delivery choice; Hide Record; Unhide Record; Save outgoing fields; Save incoming fields; Unhide Record |
| Login & workspaces | Audit contact's roster; Grant login; Revoke login; Shared workspaces; Sign out of live Organ |
| Contact file sync | Save File Sync; Check File Sync conflicts |
| Add contact | Add known contact; Read QR image; Scan with camera; Stop camera scan |
| Nearby on this Wi-Fi | Chat; Add known |
| My devices | Create my Organ; Save device name; Disable Karma on this device; Use this device as the only Karma executor; Allow this device to run Karma alongside the others; Remove device; Set cell config; Refresh device status and waiting visitors; Stop Karma on this Cell; Run permitted Karma on this Cell; Use this device as the only Karma executor; Allow this device to run Karma alongside the others |
| Add or join a device | Issue device enrolment code; Join my existing Organ; Read QR image; Scan with camera; Stop camera scan |
| Keys & backup | Export root key; Detach root key; Back up and reopen Lince |
| Network & presence | Find devices on this Wi-Fi; Save discovery and restart connections; Save peer port |
| Storage & sync | Save disk budget; Save directory; Reload sync settings |
| File copies | Choose files; Choose folder; Clear selection; Send copy; Accept and save copy; Change save folder; Decline copy; Cancel copy |
| Shared workspaces | Connect or reload workspaces; Sign in; Sign out; Open a separate permitted view; Create hosted workspace; Edit read ceiling or write grant; Preview policy admission; Add policy grant; Allow assertion additions or removals; Propose policy change; Add Text, Record or Area; Propose Record changes; Propose new Record or deletion; Propose selected Area behavior; Preview operation and Record consequences; Save draft on this computer; Recover local drafts; Discard draft; Submit operation; Review changes and history; Revise and resubmit as my change; Preview as original Actor; Approve against current revision; Reject change; Older or latest changes; Delete hosted workspace |
| Sync activity | Latest activity; Older activity; Set history retention; Clear recent history |
| Moves & pending offers | Preview complete move; Offer previewed move; Accept invitation; Decline privately; Cancel replica offer; Accept move and complete set; Decline move privately; Cancel move; Accept Transfer invitation; Reject Transfer invitation; Accept file copy; Cancel file copy |
| Mail delivery | Save mailbox copy policy; Retry recovery; Refresh outgoing mail; Collect mail now; Check saved message recovery |
| Carriers & pickup points | Answer carrying request; Use carrying invite; Refresh carried mail; Refresh requests; Refresh pickup points; Issue carrying invite; Offer to carry mail; Ask them to carry mail; Add pickup point; Send mail now; Stop carrying; Remove pickup point |
| Public discovery | Refresh My posts and services; Search local announcements; Preview saved announcement; Write a private introduction; View public Organ profile; Continue browsing announcements; Load public {key}; Review the next local conflict page |
| My announcements | Prepare from an existing Need/Contribution; Save announcement draft; Private reply key status; Prepare this device's private reply keys; Use preserved draft Record: Record; Preview; Archive ended announcement; Next My posts page; Publish this exact preview; Preview publication or update; Preview pause; Preview fulfilled; Preview withdrawal |
| Private requests | Requests and private conversations; Reset this device's live private messaging keys; Discard unreadable ciphertext; Resume this introduction on this device; Discard saved introduction; Review delivery or resume this saved message; Send private reply; Reveal my Organ profile in this conversation; Connect as known contacts; Decide request; Archive this private conversation; Unblock this private identity; Next Requests page; Resend this expired Message; Resume this saved Message on this device; Accept conversation; Decline request; Block participant; Close conversation |
| Public profile | Prepare public profile image; Save shared public profile; Reload public profile; Change public editing authority; Withdraw public profile from its hosts |
| Saved searches | Save this search filter; Saved searches and notification settings; Enable periodic checks on this device; Search these chosen directories now; Next saved filters; Review retained matches; Clear displayed matches; Remove saved filter |
| Ask contacts | Save this device's contact search participation; Refresh contact query progress; Clear removed or blocked contact's query consent; Save this contact's separate query permissions; Ask contact; Cancel this contact query; View this query's saved answers; Clear finished local contact-query history; Save this contact’s separate query permissions |
| Chosen services | Inspect a social server; Remember server roles; Inspect remembered server; Remove remembered server; Browse chosen servers; Search chosen servers |
| Public forwarding | Save this device's gossip participation; Clear retained gossip consent; Save separate contact gossip consent; Save separate contact gossip consent |
| Visibility & reports | Mute post; Remove a listing on this host; Show this target again; Next hidden targets; Restore this listing if still valid; Next removed listings; Preview a report to one operator; Send this exact report; Clear my report work; Dismiss this report; Remove this host's current listing; Next received reports; Hide this announcement; Hide this public author; Hidden announcements and authors; Review this host’s removed listings; My submitted reports; Review this operator’s report inbox |
| Host a service | Save social service roles; Refresh social service health; Rebuild public search index |
| Credits | View native QR and media attributions |

Identity/fingerprints, device mail-key expiry and permissions, hidden/refused records, carrier storage vs recipient receipt, private-message deadlines and pending publication are retained as contextual information. Empty/permission/error states are explicitly selectable. Deeper canvas and transfer interactions are sketched at their destination, not fully recreated.

Native entry points: `crates/desktop/src/organ_castle.rs`, `configuration.rs`, `owner_backup.rs`, `sync_castle.rs`, `sync_castle/blobs.rs`, `sync_castle/offers.rs`, `workspace_sync.rs`, `information/sync.rs`, and `organ_castle/social/*.rs`. The shared form model is `crates/interface/src/organ.rs`. Native action handlers remain unchanged.
