// Garde de style (branchée dans `npm run lint`) :
// 1. aucune valeur visuelle littérale hors `src/styles/tokens.css` : ni couleur (#hex, rgb(),
//    hsl()), ni longueur en px, dans les blocs <style> des composants et dans base.css ;
// 2. aucun attribut `style=` en ligne dans un gabarit ;
// 3. aucun jeton de tokens.css sans utilisateur (var(--nom) nulle part ailleurs).
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const src = join(root, "src");

function walk(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
}

const files = walk(src);
const problems = [];
// Jetons du design system réservés aux composants des tickets suivants (jauges, courbes, tableaux).
const RESERVED = new Set(["--ac2", "--cool", "--fs-label", "--fs-h2", "--ls-label"]);
const COLOR = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/;
const PX = /(?<![\w-])-?\d*\.?\d+px\b/;

for (const file of files) {
  const name = relative(root, file).replaceAll("\\", "/");
  if (name === "src/styles/tokens.css" || name === "src/styles/fonts.css") continue;
  const text = readFileSync(file, "utf8");
  let css = "";
  if (file.endsWith(".vue")) {
    for (const match of text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) css += `${match[1]}\n`;
    const template = text.match(/<template[^>]*>([\s\S]*)<\/template>/)?.[1] ?? "";
    if (/\sstyle\s*=|:style\s*=|v-bind:style/.test(template)) {
      problems.push(`${name} : attribut style en ligne`);
    }
  } else if (file.endsWith(".css")) {
    css = text;
  }
  css.split("\n").forEach((line, index) => {
    const code = line.replace(/\/\*.*?\*\//g, "");
    if (COLOR.test(code))
      problems.push(`${name} (style ${index + 1}) : couleur littérale : ${line.trim()}`);
    if (PX.test(code))
      problems.push(`${name} (style ${index + 1}) : longueur en px littérale : ${line.trim()}`);
  });
}

// Jetons déclarés mais jamais utilisés.
const tokensText = readFileSync(join(src, "styles/tokens.css"), "utf8");
const declared = [...tokensText.matchAll(/^\s*(--[\w-]+)\s*:/gm)].map((m) => m[1]);
const everything = files
  .filter((f) => /\.(vue|css|ts)$/.test(f))
  .map((f) => readFileSync(f, "utf8"))
  .join("\n");
for (const token of declared) {
  const uses = everything.split(`var(${token}`).length - 1;
  if (uses === 0 && !RESERVED.has(token))
    problems.push(`src/styles/tokens.css : jeton sans utilisateur : ${token}`);
}

if (problems.length > 0) {
  console.error("Garde de style :");
  for (const line of problems) console.error(`  ${line}`);
  process.exit(1);
}
console.log(`Garde de style OK (${declared.length} jetons, ${files.length} fichiers).`);
