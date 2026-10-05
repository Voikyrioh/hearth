// Garde de style, branchée dans `npm run lint`. Elle vérifie, dans les blocs <style> des
// composants `.vue` et dans les `.css` de `src/` (sauf `tokens.css` et `fonts.css`), qu'AUCUNE
// valeur visuelle n'est littérale :
//   - couleurs : #hex, rgb()/rgba(), hsl()/hsla(), hwb(), lab(), lch(), oklab(), oklch(), color(),
//     et les 148 couleurs nommées CSS dans une propriété de couleur ;
//   - longueurs : px, rem, em, pt, pc, cm, mm, in, ch, ex, vmin, vmax, et vw/vh hors `100vw`/`100vh` ;
//   - opacity : seulement 0 ou 1 ; line-height, z-index, font-weight, letter-spacing : jamais un
//     nombre ou un mot-clé de poids (`bold`), toujours un jeton ;
//   - durées : toute valeur en s ou ms (sauf 0).
// Sont donc ADMIS sans jeton : les pourcentages, `0`, les nombres sans unité d'autres propriétés
// (flex, grid, order…), les mots-clés (auto, none, inherit, transparent, currentColor…).
// Elle vérifie aussi qu'aucun gabarit n'a d'attribut `style=` en ligne, et qu'aucun jeton de
// `tokens.css` n'est sans utilisateur (`var(--nom)` exact, ailleurs ou dans un autre jeton).
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

const NAMED_COLORS = new Set(
  `aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet
  brown burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue
  darkcyan darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange
  darkorchid darkred darksalmon darkseagreen darkslateblue darkslategray darkslategrey darkturquoise
  darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite forestgreen
  fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew hotpink indianred
  indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan
  lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen
  lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime limegreen linen magenta
  maroon mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen mediumslateblue
  mediumspringgreen mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin
  navajowhite navy oldlace olive olivedrab orange orangered orchid palegoldenrod palegreen
  paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue purple rebeccapurple
  red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue
  slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet
  wheat white whitesmoke yellow yellowgreen`
    .split(/\s+/)
    .filter(Boolean),
);
const COLOR_PROPERTY =
  /^(color|background|background-color|border(-[a-z]+)*|outline(-color)?|fill|stroke|box-shadow|text-shadow|caret-color|accent-color|text-decoration-color)$/;

const COLOR_FUNCTION = /#[0-9a-fA-F]{3,8}\b|\b(rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(/;
const LENGTH = /(?<![\w-])-?\d*\.?\d+(px|rem|em|pt|pc|cm|mm|in|ch|ex|vmin|vmax)\b/;
const VIEWPORT = /(?<![\w-])-?\d*\.?\d+(vw|vh)\b/g;
const DURATION = /(?<![\w-])\d*\.?\d+m?s\b/g;

const files = walk(src);
const problems = [];

function checkDeclaration(name, line, property, value) {
  const where = `${name} (style ${line})`;
  const shown = `${property}: ${value}`;
  if (COLOR_FUNCTION.test(value)) problems.push(`${where} : couleur littérale : ${shown}`);
  if (COLOR_PROPERTY.test(property)) {
    const bare = value.replace(/var\([^)]*\)/g, " ");
    for (const word of bare.match(/[a-z]+/g) ?? []) {
      if (NAMED_COLORS.has(word)) problems.push(`${where} : couleur nommée « ${word} » : ${shown}`);
    }
  }
  if (LENGTH.test(value)) problems.push(`${where} : longueur littérale : ${shown}`);
  for (const match of value.matchAll(VIEWPORT)) {
    if (!/^100(vw|vh)$/.test(match[0])) problems.push(`${where} : longueur littérale : ${shown}`);
  }
  if (property === "opacity" && !/^(0|1|var\(.*\))$/.test(value.trim())) {
    problems.push(`${where} : opacity littérale (jeton, ou 0 ou 1) : ${shown}`);
  }
  if (["line-height", "z-index", "letter-spacing"].includes(property)) {
    if (!/^(auto|normal|inherit|initial|unset|0|var\(.*\))$/.test(value.trim())) {
      problems.push(`${where} : ${property} littéral : ${shown}`);
    }
  }
  if (property === "font-weight" && !/^(inherit|initial|unset|var\(.*\))$/.test(value.trim())) {
    problems.push(`${where} : font-weight littéral : ${shown}`);
  }
  for (const match of value.matchAll(DURATION)) {
    if (!/^0m?s$/.test(match[0])) problems.push(`${where} : durée littérale : ${shown}`);
  }
}

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
  } else {
    continue;
  }
  css = css.replace(/\/\*[\s\S]*?\*\//g, (comment) => comment.replace(/[^\n]/g, " "));
  css.split("\n").forEach((line, index) => {
    // Déclarations `propriété: valeur` (plusieurs possibles sur une ligne, séparées par `;`).
    for (const raw of line.split(";")) {
      const declaration = raw.slice(raw.lastIndexOf("{") + 1);
      const match = declaration.match(/^[\s{]*([a-z-]+)\s*:\s*(.+?)\s*}?\s*$/);
      if (match?.[1] && match[2]) checkDeclaration(name, index + 1, match[1], match[2]);
    }
  });
}

// Jetons déclarés mais jamais utilisés (nom exact : `--ac` n'est pas `--ac2`).
const tokensText = readFileSync(join(src, "styles/tokens.css"), "utf8");
const declared = [...tokensText.matchAll(/^\s*(--[\w-]+)\s*:/gm)].map((m) => m[1]);
const everything = files
  .filter((f) => /\.(vue|css|ts)$/.test(f))
  .map((f) => readFileSync(f, "utf8"))
  .join("\n");
for (const token of declared) {
  const used = new RegExp(`var\\(\\s*${token}\\s*[,)]`);
  if (!used.test(everything))
    problems.push(`src/styles/tokens.css : jeton sans utilisateur : ${token}`);
}

if (problems.length > 0) {
  console.error("Garde de style :");
  for (const line of problems) console.error(`  ${line}`);
  process.exit(1);
}
console.log(`Garde de style OK (${declared.length} jetons, ${files.length} fichiers).`);
