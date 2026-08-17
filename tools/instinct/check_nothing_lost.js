// Did the merge LOSE anything?
//
// A different question from "is the folder adoptable" (check_lingua.js, which
// reads preludes) and from the old check_docs_lossless.js, which compared
// against Markdown in `docs/` that no longer exists.
//
// The merge is allowed to delete text — that is most of the point. What it is
// not allowed to do is delete text SILENTLY. So this compares every passage in
// a git revision against the folder as it stands now, and anything that no
// longer appears has to be listed, by hand, in ALLOWED below with the reason.
// A missing passage with no entry fails the run. The revision to compare
// against is the one the merge STARTED from — e67f494 — not HEAD; comparing
// against HEAD only ever proves the last step was safe.
//
//     node tools/instinct/check_nothing_lost.js <rev> [dir]
//
// Passages are compared whitespace-normalised and include SHORT lines —
// headings especially. A first pass here skipped anything under sixty
// characters and quietly dropped a `# ESTATUTO SOCIAL` heading, which is
// exactly the kind of structure a length threshold hides.

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const rev = process.argv[2] || "HEAD";
const dir = process.argv[3] || "docs/records";

// Whole CLASSES of deletion that are mechanical rather than editorial. Each is
// a shape the restructure produced everywhere at once, so listing every
// instance would be noise standing in for review rather than review. A rule
// still has to match a shape narrowly enough that no prose can hide inside it.
//
// There was briefly a fourth rule here excusing ANY heading under sixty
// characters. It made the gate pass while `## ESTATUTO SOCIAL` was deleted —
// the exact loss this file was written to catch. A rule has to name a shape
// that carries no meaning of its own; "short" is not one.
const RULES = [
  [/^#{1,3} \d+[a-c]?\. /, "renamed", "the section number was a coordinate; the tree carries the order"],
  [/^> Chapter \d+ — /, "renamed", "same, for the chapter-number line above a title"],
  [/^#{1,3} \[[ x]\] /, "renamed", "a heading is not a checkbox"],
];

// Each entry is [a distinctive fragment of the passage, which pile it went to,
// and where it went or why it is gone]. `stable` never appears here — stable
// text is still in the folder, so it never goes missing in the first place.
const ALLOWED = [
  ["## \"If\" is the Condition", "renamed", "merged into Condition, teaching above the specification"],
  ["## \"And\" is the Threshold", "renamed", "merged into Threshold"],
  ["## \"Then\" is the Consequence", "renamed", "merged into Consequence"],
  ["## A monthly example", "renamed", "merged into Frequency, which is what the example teaches"],
  ["## Core Model", "renamed", "merged into What a Transfer actually is"],
  ["## MVP Build Order", "renamed", "became the first section of Building Transfer"],
  ["## OLD STUFF DOWN HERE", "renamed", "it was never old stuff; renamed Transfer, in theory"],
  ["# Theory", "renamed", "same passage, one heading instead of two"],
  ["## What a Transfer actually is", "renamed", "kept the name, gained the Core Model spec below it"],
  ["## How to read this file", "renamed", "the durable half moved to How we work"],
  ["# Links", "renamed", "the subject is Assertion; the link is what an assertion looks like"],
  ["# Cells & Organs", "renamed", "the subject is Organ"],
  ["# Transfers", "renamed", "singular, like every other noun in the tree"],
  ["Records get useful when they point at each other", "stable", "rewritten as the opening of Assertion"],
  ["Records get their meaning from the concepts applied to them", "stable", "rewritten as the opening of Concept"],
  ["Every record so far lived alone on one machine", "stable", "rewritten as the opening of Organ"],
  ["Transfers are how the two halves of a match", "superseded", "rewritten as the opening of Transfer"],
  ["Rules that watch your records and act on them", "superseded", "rewritten as the opening of Karma"],
  ["Every feature is written the same way", "stable", "moved to How we work"],
  ["Sections are in build order", "superseded", "described a numbered-section scheme the tree replaced"],
  ["The frontend is the first thing you touch", "superseded", "rewritten as the root of the tree in First Steps"],
  ["The unit everything else in Lince is made of", "superseded", "rewritten as the opening of Record"],
  ["Ontology is Lince's specification for modeling information", "superseded", "rewritten as the opening of Ontology"],
  ["Everything else — Protein, Sync, CRDT, federation", "superseded", "rewritten in Ontology; CRDT and federation are named in their own branches"],
  ["A **Record** is one thing that matters to an Organ", "superseded", "rewritten as the opening of Record"],
  ["- [x] Core fields above", "stable", "every finished item was folded into the prose of What a Record holds"],
  ["## INSTITUTO LINCE", "stable", "renamed to the Instituto Lince heading"],
  ["## Interfaceless - The Death of Lince", "stable", "renamed to The Death of Lince"],
  ["## 1. Record: a modeled thing", "stable", "renamed to Record, without the section number"],
  ["— Introduction", "superseded", "chapter numbers are coordinates; the tree carries the order"],
  ["> Chapter 2 — The unit", "superseded", "chapter numbers are coordinates; the tree carries the order"],
  ["# Records", "superseded", "merged into Record, singular, with the ontology section"],
  ["# Lince, the institute", "stable", "moved to Instituto Lince, under its own heading"],
  ["# Merch", "stable", "promoted to its own Record, Merch"],
  ["`@chapter [[Records|uid]] 3`", "superseded", "the example named the old parent predicate"],
];

const norm = (s) => s.replace(/\s+/g, " ").trim();

const corpus = norm(
  fs
    .readdirSync(dir)
    .filter((f) => f.endsWith(".lingua"))
    .map((f) => fs.readFileSync(path.join(dir, f), "utf8"))
    .join("\n"),
);

const was = execFileSync("git", ["ls-tree", "-r", "-z", "--name-only", rev, dir], {
  encoding: "utf8",
}).split("\0").filter(Boolean);

let checked = 0;
const unexplained = [];
const used = new Set();
let renamed = 0;

for (const file of was) {
  const text = execFileSync("git", ["show", `${rev}:${file}`], { encoding: "utf8" });
  // The prelude is structure, not prose; check_lingua.js owns it.
  const body = text.split(/\n---\n/).slice(1).join("\n---\n");
  for (const passage of body.split(/\n\s*\n/)) {
    const p = norm(passage);
    if (!p) continue;
    checked++;
    if (corpus.includes(p)) continue;
    const excuse = ALLOWED.find(([fragment]) => p.includes(fragment));
    if (excuse) {
      used.add(excuse[0]);
      continue;
    }
    if (RULES.some(([shape]) => shape.test(p))) {
      renamed++;
      continue;
    }
    unexplained.push([file.replace(`${dir}/`, ""), p]);
  }
}

for (const [fragment] of ALLOWED) {
  if (!used.has(fragment)) {
    console.log(`stale excuse: "${fragment}" is listed as gone but is still there`);
  }
}

for (const [file, passage] of unexplained) {
  console.log(`\nLOST  ${file}\n      ${passage.slice(0, 160)}`);
}

const stale = ALLOWED.filter(([f]) => !used.has(f)).length;
console.log(
  `\n${checked} passages at ${rev}, ${unexplained.length} unexplained, ` +
    `${ALLOWED.length - stale} deletions accounted for, ${renamed} renamed, ${stale} stale`,
);
process.exit(unexplained.length || stale ? 1 : 0);
