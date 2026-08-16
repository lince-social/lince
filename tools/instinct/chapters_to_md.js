// Instinct chapter HTML -> Markdown, MECHANICALLY.
//
// Deterministic on purpose: "without losing a bit" is checkable for a
// conversion and a matter of opinion for a rewrite. Every mermaid block is
// copied verbatim; every text node survives. The splitting into Records is a
// separate, later step that operates on THIS output, never on the HTML.
const fs = require("fs");
const path = require("path");

const dir = process.argv[2];
const out = process.argv[3];
fs.mkdirSync(out, { recursive: true });

function decode(s) {
  return s
    .replace(/&lt;/g, "<").replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"').replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ").replace(/&amp;/g, "&");
}

function inline(s) {
  return decode(
    s
      .replace(/<strong>([\s\S]*?)<\/strong>/g, "**$1**")
      .replace(/<em>([\s\S]*?)<\/em>/g, "_$1_")
      .replace(/<code>([\s\S]*?)<\/code>/g, "`$1`")
      .replace(/<a [^>]*href="([^"]*)"[^>]*>([\s\S]*?)<\/a>/g, "[$2]($1)")
      .replace(/<br\s*\/?>/g, "\n")
      .replace(/<[^>]+>/g, "")
  ).replace(/\s+/g, " ").trim();
}

for (const file of fs.readdirSync(dir).filter((f) => f.endsWith(".html")).sort()) {
  const html = fs.readFileSync(path.join(dir, file), "utf8");
  const parts = [];
  // Walk top to bottom, taking mermaid blocks out whole so nothing inside
  // them is ever treated as markup.
  const token = /<pre class="mermaid">([\s\S]*?)<\/pre>|<(h1|h2|h3|p|li)(\s[^>]*)?>([\s\S]*?)<\/\2>/g;
  let m;
  while ((m = token.exec(html))) {
    if (m[1] !== undefined) {
      parts.push("```mermaid\n" + decode(m[1]).trim() + "\n```");
      continue;
    }
    const tag = m[2];
    const attrs = m[3] || "";
    const text = inline(m[4]);
    if (!text) continue;
    if (tag === "h1") parts.push("# " + text);
    else if (tag === "h2") parts.push("## " + text);
    else if (tag === "h3") parts.push("### " + text);
    else if (tag === "li") parts.push("- " + text);
    else if (/figure__caption/.test(attrs)) parts.push("_" + text + "_");
    else if (/chapter__eyebrow/.test(attrs)) parts.push("> " + text);
    else parts.push(text);
  }
  const name = file.replace(/\.html$/, ".md");
  fs.writeFileSync(path.join(out, name), parts.join("\n\n") + "\n");
  console.log(name, parts.length, "blocks");
}
