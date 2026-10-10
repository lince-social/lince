# View the Organ structure study

Run from the repository root:

```sh
cargo xtask design serve organ-management-structure --port 6180
```

Open http://127.0.0.1:6180 in your browser. Keep that command running; Ctrl+C stops it. A terminal that already has a preview running on this port does not need another server. If the port is occupied by something else, choose `--port 6181` and open that port instead.

Use Alternative to compare all three or inspect one. Use Viewport for Desktop, Mobile, or Sand; Theme changes all previews using Rust-generated tokens. Save a study file to refresh the comparison automatically.

1. Three destinations: persistent My Organ / Contacts / Connect navigation, with focused pages below each destination.
2. Task hub: my Organ, three actions, and contacts on the opening page; Manage opens the grouped tools directory.
3. Two spaces: My Organ / People navigation and a page picker within each space.

Click Ana, open Sharing & trust, and compare the path to a less frequent task such as Keys & backup or Chosen services. Expand an individual control to see its fields. Prototype scenario at the bottom of each preview switches fresh, pending, offline, viewing-only, expired, and large-list data. The scenario control is part of the study viewer experience, not a proposed production control.

Direct previews are `/alternative-1.html`, `/alternative-2.html`, and `/alternative-3.html` on the same server. These are interactive HTML sketches with synthetic data. Camera/image scanning, native windows, cryptography, file operations and backend actions are simulated or represented by entry points. The QR block illustrates placement and is not scannable.

The recommendation is the Task hub for the requested opening priority. Selection remains pending in `decision.md`; choose an alternative or name specific parts to combine before Rust implementation.
