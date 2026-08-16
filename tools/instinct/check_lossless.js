// Did the conversion lose anything?
//
// A diff, not a judgement. Two questions, both countable:
//   1. Every mermaid block in the HTML appears VERBATIM in the output, and the
//      per-chapter counts match.
//   2. Every sentence of visible prose in the HTML appears somewhere in the
//      output.
//
// Run against the chapter Markdown first, and later against the split
// `.lingua` Records — the second argument is just "a directory of text".
const fs = require("fs");
const path = require("path");

const htmlDir = process.argv[2];
const outDir = process.argv[3];

function decode(s) {
  return s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'").replace(/&nbsp;/g, " ").replace(/&amp;/g, "&");
}
// Both sides are reduced to the same canonical form before comparing, so the
// check is about CONTENT and not about whether `@food` kept its backticks.
// Anything that only ever came from markup or from Markdown emphasis goes.
const norm = (s) =>
  s
    .replace(/<br\s*\/?>/g, " ")
    .replace(/<[^>]+>/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[`*_“”‘’]/g, "")
    .replace(/\s+/g, " ")
    .replace(/\s+([,.;:!?])/g, "$1")
    .trim();

const haystack = fs.readdirSync(outDir)
  .map((f) => fs.readFileSync(path.join(outDir, f), "utf8"))
  .join("\n\n");
const flatHay = norm(haystack);

let failures = 0;
let sentences = 0;
let diagrams = 0;

for (const file of fs.readdirSync(htmlDir).filter((f) => f.endsWith(".html")).sort()) {
  const html = fs.readFileSync(path.join(htmlDir, file), "utf8");

  const blocks = [...html.matchAll(/<pre class="mermaid">([\s\S]*?)<\/pre>/g)]
    .map((m) => decode(m[1]).trim());
  for (const block of blocks) {
    diagrams++;
    if (!haystack.includes(block)) {
      failures++;
      console.log(`MISSING DIAGRAM  ${file}: ${block.split("\n")[0]}`);
    }
  }

  // Visible prose: strip the mermaid blocks (already checked verbatim), then
  // take the text of every paragraph-ish element.
  const prose = html.replace(/<pre class="mermaid">[\s\S]*?<\/pre>/g, "");
  for (const m of prose.matchAll(/<(h1|h2|h3|p|li)(?:\s[^>]*)?>([\s\S]*?)<\/\1>/g)) {
    const text = norm(decode(m[2]));
    if (!text) continue;
    for (const raw of text.split(/(?<=[.?!:])\s+(?=[A-Z"'])/)) {
      const sentence = norm(raw);
      if (sentence.length < 12) continue; // a fragment matches everything
      sentences++;
      if (!flatHay.includes(sentence)) {
        failures++;
        console.log(`MISSING TEXT     ${file}: ${sentence.slice(0, 90)}`);
      }
    }
  }
}

console.log(`\n${diagrams} diagrams, ${sentences} sentences, ${failures} missing`);
process.exit(failures ? 1 : 0);
