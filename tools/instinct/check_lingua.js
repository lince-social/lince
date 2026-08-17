// Is this folder ADOPTABLE? A different question from "is it lossless".
//
// File Sync refuses a file that names an unknown predicate, links to a uid
// that is not there, or claims a uid another file already claims. A folder can
// be perfectly faithful to its source and still be refused in full, so this
// checks the things the tick will check, before the tick runs.
const fs = require("fs");
const path = require("path");

const dir = process.argv[2];
// One tree, one parent link: `@part-of [[Idea|uid]] n`, where n orders the
// siblings and may be a decimal so an insertion renumbers nothing.
// `@reference` is the only other link and it points sideways, never down.
// `@chapter` and `@see-also` are still accepted while the corpus is converted
// a subject at a time. The list stays frozen: every addition is another
// Concept somebody has to create before the folder can be adopted.
const ALLOWED = new Set([
  "idea", "chapter", "position", "see-also", "reference",
  "document", "section", "task", "part-of",
  "instinct",
  "stable", "backlog", "todo", "wip",
]);

// State is the quantity AND a Concept, which is two projections of one fact
// rather than two authorities. The number is what sorts — "active work" is
// `quantity < -1`, a comparison no set of unordered words can express — and
// the Concept is what a sand filters and colours. So the word is checked
// against the number rather than trusted: a file saying `@wip` at quantity 1
// would show up as doing on a board and as finished in a query.
const STATE = new Map([["1", "stable"], ["0", "backlog"], ["-1", "todo"], ["-2", "wip"]]);
const STATES = new Set(STATE.values());

// `@instinct` is what File Sync selects on, so a file without it silently
// stops being mirrored — it does not fail, it just quietly leaves the folder.
// Checked here rather than trusted.
const REQUIRED = "instinct";
const UID = /^r_[0-9A-HJKMNP-TV-Z]{26}$/;

const files = fs.readdirSync(dir).filter((f) => f.endsWith(".lingua")).sort();
const byUid = new Map();
const records = [];
let problems = 0;
const bad = (file, msg) => { problems++; console.log(`${file}: ${msg}`); };

for (const file of files) {
  const text = fs.readFileSync(path.join(dir, file), "utf8");
  const m = text.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) { bad(file, "no metadata block"); continue; }
  const rec = { file, title: file.replace(/\.lingua$/, ""), uid: null, links: [], quantity: null, marked: false };
  for (const raw of m[1].split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    if (line.startsWith("uid:")) { rec.uid = line.slice(4).trim(); continue; }
    if (line.startsWith("quantity:")) { rec.quantity = line.slice(9).trim(); continue; }
    if (!line.startsWith("@")) { bad(file, `not a Lingua line: ${line}`); continue; }
    const predicate = line.replace(/^@@?/, "").split(/\s/)[0];
    if (predicate === REQUIRED) rec.marked = true;
    if (STATES.has(predicate)) rec.state = predicate;
    if (!ALLOWED.has(predicate)) bad(file, `predicate @${predicate} is not in the frozen list`);
    for (const link of line.matchAll(/\[\[([^\]|]*)\|([^\]]*)\]\]/g)) {
      rec.links.push({ title: link[1], uid: link[2] });
    }
    if (/\[\[[^\]|]*\]\]/.test(line)) bad(file, `a link with no uid: ${line}`);
  }
  if (!rec.uid) bad(file, "no uid — cross-links need one minted in advance");
  else if (!UID.test(rec.uid)) bad(file, `uid ${rec.uid} is malformed`);
  else if (byUid.has(rec.uid)) bad(file, `uid collides with ${byUid.get(rec.uid)}`);
  else byUid.set(rec.uid, file);
  if (rec.quantity === null) bad(file, "no quantity — state is the quantity");
  else if (rec.state && STATE.get(rec.quantity) !== rec.state) {
    const expected = STATE.get(rec.quantity);
    bad(
      file,
      `says @${rec.state} but quantity ${rec.quantity} means ` +
        (expected ? `@${expected}` : "no state word at all"),
    );
  }
  if (!rec.marked) {
    bad(file, `no @${REQUIRED} — File Sync selects on it, so this file would stop being mirrored`);
  }
  records.push(rec);
}

for (const rec of records) {
  for (const link of rec.links) {
    const target = byUid.get(link.uid);
    if (!target) { bad(rec.file, `links to ${link.uid}, which no file in this folder claims`); continue; }
    const title = target.replace(/\.lingua$/, "");
    if (title !== link.title) {
      bad(rec.file, `link says "${link.title}" but ${link.uid} is "${title}"`);
    }
  }
}

console.log(`\n${files.length} files, ${byUid.size} uids, ${problems} problems`);
process.exit(problems ? 1 : 0);
