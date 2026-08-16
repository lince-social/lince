// docs/*.md -> docs/records/*.lingua, deterministically.
//
// The last Markdown in `docs/` becomes Records. Mechanical on purpose, for the
// same reason the Instinct conversion was: "nothing was lost" is checkable for
// a conversion and a matter of opinion for a rewrite, and these files are the
// project's own reasoning — the part that cannot be recovered from the code.
//
// The split follows the documents' own structure:
//
//   the file            -> one `@@document` Record, `@position` among the docs
//   each `## ` section  -> one `@@section` Record, `@part-of [[Doc|uid]] n`
//   open checkboxes     -> one `@@task` Record per section holding them all,
//                          `quantity: -1`, because a cluster of boxes under one
//                          heading is what a person actually picks up. `- [x]`
//                          boxes are DONE and stay in the section body with the
//                          prose that explains them.
//
// Uids are derived from the document name and the heading with a hash, not
// minted randomly: re-running this must produce the same uids or every link in
// the folder breaks and the re-run reads as 200 deletions and 200 creations.
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

// A uid is a stable function of what the Record IS. Same input, same uid,
// forever — which is what makes re-running this an update rather than a
// replacement.
function uidFor(key) {
  const digest = crypto.createHash("sha256").update(key).digest();
  let out = "";
  for (let i = 0; i < 26; i++) out += ALPHABET[digest[i] % 32];
  return "r_" + out;
}

// Filenames become heads, so they may not carry the characters a path uses.
function safeName(title) {
  return title
    .replace(/[`*_[\]]/g, "")
    .replace(/[/\\:<>"|?]/g, "-")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 90);
}

function link(title, uid) {
  return `[[${title}|${uid}]]`;
}

const docsDir = process.argv[2] || "docs";
const outDir = process.argv[3] || "docs/records";
const ORDER = ["Lince", "Ontology", "Karma", "Transfer", "Interface", "thoughts"];

// Every Record in this folder carries it, and File Sync SELECTS on it: a file
// without it is silently not mirrored rather than loudly wrong.
const MARK = "@instinct";

const used = new Map();
let written = 0;

// The filename IS the Record's head, so two Records cannot share one. Both
// halves of that are enforced here, and the second was learned the hard way:
// `docs/Karma.md` wanted `Karma.lingua`, which is the Instinct chapter on
// Karma, and writing it silently destroyed a chapter while leaving its six
// ideas pointing at a uid nothing claimed. A converter that overwrites files
// it did not create is a converter that eats hand-written work.
function write(name, prelude, body) {
  const file = path.join(outDir, `${name}.lingua`);
  if (used.has(name)) throw new Error(`two Records want the name ${name}`);
  if (fs.existsSync(file)) {
    throw new Error(`${name}.lingua already exists and was not written by this run`);
  }
  used.set(name, true);
  fs.writeFileSync(file, `---\n${prelude.join("\n")}\n---\n\n${body.replace(/\n+$/, "")}\n`);
  written++;
}

for (const docName of ORDER) {
  const source = path.join(docsDir, `${docName}.md`);
  if (!fs.existsSync(source)) continue;
  const text = fs.readFileSync(source, "utf8");
  const docUid = uidFor(`document:${docName}`);
  // "(document)" is not decoration: `docs/Karma.md` and the Instinct chapter
  // on Karma both want to be called Karma, and they are different Records.
  const docTitle = safeName(`${docName} (document)`);

  // Split on `## ` at the start of a line, keeping everything before the first
  // one as the document's own body.
  const lines = text.split("\n");
  const sections = [];
  let current = { title: null, lines: [] };
  let inFence = false;
  for (const line of lines) {
    if (/^```/.test(line)) inFence = !inFence;
    if (!inFence && /^## +\S/.test(line)) {
      sections.push(current);
      current = { title: line.replace(/^## +/, "").trim(), lines: [] };
      continue;
    }
    current.lines.push(line);
  }
  sections.push(current);

  const head = sections.shift();
  write(docTitle, [`uid: ${docUid}`, "@@document", MARK, `@position ${ORDER.indexOf(docName) + 1}`, "quantity: 1"],
    head.lines.join("\n").trim() || `# ${docTitle}`);

  sections.forEach((section, index) => {
    const title = section.title;
    const sectionUid = uidFor(`section:${docName}:${title}`);
    const name = safeName(`${docName} - ${title}`);

    // Open boxes leave the section and become one task Record; done boxes and
    // everything else stay, because prose plus a ticked box is the record of
    // what was built and why.
    const prose = [];
    const open = [];
    let capturing = false;
    let fence = false;
    for (const line of section.lines) {
      if (/^```/.test(line)) fence = !fence;
      if (!fence && /^ *- \[ \]/.test(line)) { capturing = true; open.push(line); continue; }
      if (capturing && !fence && /^ {2,}\S/.test(line)) { open.push(line); continue; }
      if (capturing && line.trim() === "" && open.length) { open.push(line); continue; }
      capturing = false;
      prose.push(line);
    }

    write(name, [
      `uid: ${sectionUid}`,
      "@@section",
      MARK,
      `@part-of ${link(docTitle, docUid)} ${index + 1}`,
      "quantity: 1",
    ], `## ${title}\n\n${prose.join("\n").trim()}`);

    const openText = open.join("\n").trim();
    if (openText) {
      const taskUid = uidFor(`task:${docName}:${title}`);
      write(safeName(`TODO - ${docName} - ${title}`), [
        `uid: ${taskUid}`,
        "@@task",
        MARK,
        `@part-of ${link(name, sectionUid)} ${index + 1}`,
        "quantity: -1",
      ], `## ${title}\n\n${openText}`);
    }
  });
}

console.log(`${written} .lingua files written to ${outDir}`);
