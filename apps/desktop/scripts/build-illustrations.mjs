// Produit les illustrations des écrans vides depuis les images générées (HRT-31, ADR-0027) : fond
// rendu transparent, réduites à deux fois la taille d'affichage. Aucune dépendance de plus : le
// décodage et la réduction se font dans le Chromium de Playwright (comme `build-icons.mjs`), l'alpha
// et le PNG sont calculés ici.
//
//   npm run build:illustrations   lit src/assets/illustrations/source/, écrit src/assets/illustrations/*.png
//                                 et illustrations.sha256.json
//
// Méthode du fond : l'image est un dessin clair sur un fond sombre presque uni (bruit de compression
// de 3 à 4 niveaux). Pour chaque pixel p et le fond f (médiane de la bordure), l'alpha est la part
// de la distance au blanc qu'il a franchie : a = max sur les canaux de (p - f) / (255 - f), moins un
// seuil bas (ALPHA_FLOOR) pour le bruit. La couleur est celle du dessin SANS le fond : c = f + (p - f) / a.
// Reposé sur le fond f, le pixel redonne exactement p ; sur un autre fond, aucun halo : un bord
// anti-crénelé garde la couleur du trait, pas un mélange avec le sombre. Lueurs et dégradés restent
// des alphas partiels (pas de seuil dur).
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";
import { chromium } from "@playwright/test";
import { ALPHA_FLOOR, ILLUSTRATIONS, OUTPUT_SCALE, RENDER_VERSION } from "./illustration-spec.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const dir = join(root, "src", "assets", "illustrations");
const source = join(dir, "source");

export const sha256 = (buffer) => createHash("sha256").update(buffer).digest("hex");

const CRC = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) c = (CRC[(c ^ byte) & 0xff] ?? 0) ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, body) {
  const out = Buffer.alloc(12 + body.length);
  out.writeUInt32BE(body.length, 0);
  out.write(type, 4, "ascii");
  body.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + body.length)), 8 + body.length);
  return out;
}

/** PNG RGBA 8 bits, filtre choisi par ligne (somme minimale des valeurs absolues), deflate au maximum. */
export function encodePng(width, height, rgba) {
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  const abs = (v) => (v < 128 ? v : 256 - v);
  for (let y = 0; y < height; y += 1) {
    const row = rgba.subarray(y * stride, (y + 1) * stride);
    const up = y > 0 ? rgba.subarray((y - 1) * stride, y * stride) : null;
    const candidates = [0, 1, 2, 3, 4].map((type) => {
      const out = Buffer.alloc(stride);
      let sum = 0;
      for (let i = 0; i < stride; i += 1) {
        const a = i >= 4 ? row[i - 4] : 0;
        const b = up ? up[i] : 0;
        const c = up && i >= 4 ? up[i - 4] : 0;
        let predictor = 0;
        if (type === 1) predictor = a;
        else if (type === 2) predictor = b;
        else if (type === 3) predictor = (a + b) >> 1;
        else if (type === 4) {
          const p = a + b - c;
          const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
          predictor = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
        }
        out[i] = (row[i] - predictor) & 0xff;
        sum += abs(out[i]);
      }
      return { type, out, sum };
    });
    const best = candidates.reduce((x, y) => (y.sum < x.sum ? y : x));
    raw[y * (stride + 1)] = best.type;
    best.out.copy(raw, y * (stride + 1) + 1);
  }
  const head = Buffer.alloc(13);
  head.writeUInt32BE(width, 0);
  head.writeUInt32BE(height, 4);
  head[8] = 8;
  head[9] = 6;
  return Buffer.concat([
    Buffer.from("89504e470d0a1a0a", "hex"),
    chunk("IHDR", head),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** Fond estimé : médiane par canal des pixels de la bordure (6 % de chaque côté). */
export function estimateBackground(rgb, size) {
  const edge = Math.max(2, Math.round(size * 0.06));
  const channels = [[], [], []];
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      if (y >= edge && y < size - edge && x >= edge && x < size - edge) continue;
      for (let k = 0; k < 3; k += 1) channels[k].push(rgb[(y * size + x) * 4 + k]);
    }
  }
  return channels.map((values) => {
    values.sort((a, b) => a - b);
    return values[values.length >> 1];
  });
}

/** RGBA (alpha droit, couleur sans le fond) à partir des pixels opaques `rgb` (RGBA, alpha 255). */
export function removeBackground(rgb, size, floor = ALPHA_FLOOR) {
  const bg = estimateBackground(rgb, size);
  const out = Buffer.alloc(size * size * 4);
  for (let i = 0; i < size * size * 4; i += 4) {
    let a = 0;
    for (let k = 0; k < 3; k += 1) a = Math.max(a, (rgb[i + k] - bg[k]) / (255 - bg[k]));
    a = Math.min(1, Math.max(0, (a - floor) / (1 - floor)));
    if (a === 0) continue; // transparent : RVB à zéro (se compresse mieux)
    for (let k = 0; k < 3; k += 1) {
      const color = bg[k] + (rgb[i + k] - bg[k]) / a;
      out[i + k] = Math.min(255, Math.max(0, Math.round(color)));
    }
    out[i + 3] = Math.round(a * 255);
  }
  return { rgba: out, background: bg };
}

const write = (path, data) => {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, data);
};

async function main() {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.setContent("<canvas></canvas>");
  const outputs = {};
  const sources = {};
  for (const { name, source: file, display } of ILLUSTRATIONS) {
    const input = readFileSync(join(source, file));
    const size = display * OUTPUT_SCALE;
    // Décodage et réduction opaques (jamais d'alpha dans Chromium : pas de prémultiplication).
    const pixels = await page.evaluate(
      async ([b64, px]) => {
        const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
        const bitmap = await createImageBitmap(new Blob([bytes]));
        const canvas = document.createElement("canvas");
        canvas.width = px;
        canvas.height = px;
        const ctx = canvas.getContext("2d");
        ctx.imageSmoothingQuality = "high";
        ctx.drawImage(bitmap, 0, 0, px, px);
        return Array.from(ctx.getImageData(0, 0, px, px).data);
      },
      [input.toString("base64"), size],
    );
    const { rgba, background } = removeBackground(Uint8Array.from(pixels), size);
    const png = encodePng(size, size, rgba);
    write(join(dir, `${name}.png`), png);
    sources[`src/assets/illustrations/source/${file}`] = sha256(input);
    outputs[`src/assets/illustrations/${name}.png`] = {
      sha256: sha256(png),
      from: [`src/assets/illustrations/source/${file}`],
      size,
    };
    console.log(`${name}.png ${size}x${size} ${png.length} o (fond ${background.join(",")})`);
  }
  await browser.close();
  // Fins de ligne normalisées pour le texte (checkout Windows ou Linux, même empreinte).
  const spec = readFileSync(join(root, "scripts", "illustration-spec.mjs"), "utf8");
  sources["scripts/illustration-spec.mjs"] = sha256(spec.replaceAll("\r\n", "\n"));
  const record = { renderVersion: RENDER_VERSION, sources, outputs };
  write(join(dir, "illustrations.sha256.json"), `${JSON.stringify(record, null, 2)}\n`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
