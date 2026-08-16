// Did docs/*.md survive the move into docs/records/*.lingua?
//
// The same shape of check as tools/instinct/check_lossless.js and for the same
// reason: these files hold the project's reasoning, and losing a paragraph is
// silent. Every non-blank line of the source must appear in some Record body.
//
// Compares on the RAW line, not on a canonical form, because this conversion
// copies text verbatim rather than translating markup. A missing line here is
// a real loss, not a formatting difference.
const fs = require("fs");
const path = require("path");

const docsDir = process.argv[2] || "docs";
const outDir = process.argv[3] || "docs/records";

const haystack = new Set();
for (const file of fs.readdirSync(outDir).filter((f) => f.endsWith(".lingua"))) {
  for (const line of fs.readFileSync(path.join(outDir, file), "utf8").split("\n")) {
    haystack.add(line.trimEnd());
  }
}

let checked = 0;
let missing = 0;
for (const file of fs.readdirSync(docsDir).filter((f) => f.endsWith(".md"))) {
  for (const raw of fs.readFileSync(path.join(docsDir, file), "utf8").split("\n")) {
    const line = raw.trimEnd();
    if (!line.trim()) continue;
    checked++;
    if (!haystack.has(line)) {
      missing++;
      if (missing <= 15) console.log(`MISSING  ${file}: ${line.slice(0, 100)}`);
    }
  }
}

console.log(`\n${checked} lines checked, ${missing} missing`);
process.exit(missing ? 1 : 0);
