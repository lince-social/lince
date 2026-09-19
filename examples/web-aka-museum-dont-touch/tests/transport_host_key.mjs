// Which Cell does a card's binding actually reach?
//
// The board spells "our own Cell" as the EMPTY string, but the local Organ
// also has a real uid and it is the first row `/organ` returns. Any non-empty
// binding is dialled through `/live/{uid}/connect`, which asks for a CONTACT —
// so a card holding the local uid, or a uid left over from a wiped data
// directory, sent every write to a Cell that is not a contact of itself and
// failed with "not a contact". The host picker showed "Local Lince" the whole
// time, because neither uid appears among the remotes it lists.
//
// This checks the collapsing rule at the one function every caller goes
// through. It runs against the real module, not a copy.
import assert from "node:assert/strict";
import { getTransportFor, setLocalOrganId } from "../static/presentation/board/transport.js";

// `getTransportFor` opens a websocket, which there is no server for here. The
// identity of the connection it hands back is all this test is about, and
// connections are cached by their resolved key — so two ids that resolve to
// the same Cell hand back the SAME object, and two that do not, do not.
globalThis.WebSocket = class {
  constructor() {
    this.readyState = 0;
  }
  send() {}
  close() {}
  addEventListener() {}
};
globalThis.window = { location: { protocol: "http:", host: "localhost" } };

const LOCAL = "r_LOCALORGANUID00000000000A";
const CONTACT = "r_CONTACTORGANUID000000000B";
const STALE = "r_WIPEDDATADIRLEFTTHIS00000C";

setLocalOrganId(LOCAL, [CONTACT]);

const own = getTransportFor("");

assert.equal(
  getTransportFor(LOCAL),
  own,
  "the local Organ's own uid is our own Cell, not a contact to dial",
);
assert.equal(
  getTransportFor(STALE),
  own,
  "a binding to an Organ this Cell has never heard of falls back to our own Cell",
);
assert.notEqual(
  getTransportFor(CONTACT),
  own,
  "a real contact is still dialled as a remote host",
);

// Before `/organ` has answered, an unknown id must NOT be downgraded: doing so
// would send a genuinely remote sand's writes into this Cell, which is worse
// than the bug this all fixes.
const fresh = await import(
  `../static/presentation/board/transport.js?nocache=${Date.now()}`
);
fresh.setLocalOrganId(LOCAL);
assert.notEqual(
  fresh.getTransportFor(CONTACT),
  fresh.getTransportFor(""),
  "with no host list yet, a remote binding is left alone rather than treated as local",
);

console.log("ok - a card's binding resolves to the Cell it names");
