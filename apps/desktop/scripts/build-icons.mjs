// Génère les icônes de l'application, de l'installateur et de la zone de notification depuis les
// SVG de `src-tauri/icons/source/` (HRT-19, ADR-0027). Aucune dépendance de plus : le rendu se
// fait dans le Chromium de Playwright (déjà là pour les tests de bout en bout), les trames ICO
// (PNG) et le BMP de l'installateur sont assemblés ici.
//
//   npm run build:icons          réécrit les fichiers de `src-tauri/icons/` et `src-tauri/installer/sidebar.bmp`
//
// Les fichiers générés sont versionnés (le build Tauri les lit tels quels) ; un test relit leurs
// dimensions et trames. Changer un dessin : remplacer le SVG source, relancer, commiter.
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";
import { ICO_FRAMES, TRAY_SIZES, TRAY_STATES } from "./icon-spec.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const icons = join(root, "src-tauri", "icons");
const source = join(icons, "source");
const installer = join(root, "src-tauri", "installer");

const svgUri = (name) =>
  `data:image/svg+xml;base64,${readFileSync(join(source, name)).toString("base64")}`;

/** ICO « PNG dans ICO » : en-tête, une entrée par trame, puis les PNG. 256 s'écrit 0. */
export function buildIco(frames) {
  const head = Buffer.alloc(6 + 16 * frames.length);
  head.writeUInt16LE(0, 0);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(frames.length, 4);
  let offset = head.length;
  frames.forEach(({ size, png }, i) => {
    const at = 6 + 16 * i;
    head.writeUInt8(size === 256 ? 0 : size, at);
    head.writeUInt8(size === 256 ? 0 : size, at + 1);
    head.writeUInt16LE(1, at + 4); // plans
    head.writeUInt16LE(32, at + 6); // bits par pixel
    head.writeUInt32LE(png.length, at + 8);
    head.writeUInt32LE(offset, at + 12);
    offset += png.length;
  });
  return Buffer.concat([head, ...frames.map((f) => f.png)]);
}

/** BMP 24 bits, lignes de bas en haut, remplies à 4 octets (ce que lit NSIS). */
export function buildBmp(width, height, rgba) {
  const stride = Math.ceil((width * 3) / 4) * 4;
  const out = Buffer.alloc(54 + stride * height);
  out.write("BM", 0, "ascii");
  out.writeUInt32LE(out.length, 2);
  out.writeUInt32LE(54, 10);
  out.writeUInt32LE(40, 14);
  out.writeInt32LE(width, 18);
  out.writeInt32LE(height, 22);
  out.writeUInt16LE(1, 26);
  out.writeUInt16LE(24, 28);
  out.writeUInt32LE(stride * height, 34);
  out.writeInt32LE(2835, 38);
  out.writeInt32LE(2835, 42);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const from = ((height - 1 - y) * width + x) * 4;
      const to = 54 + y * stride + x * 3;
      out[to] = rgba[from + 2];
      out[to + 1] = rgba[from + 1];
      out[to + 2] = rgba[from];
    }
  }
  return out;
}

const write = (path, data) => {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, data);
};

async function main() {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.setContent("<canvas id=c></canvas>");

  /** PNG (Buffer) d'un SVG rendu à size × size. */
  const png = async (svg, size) => {
    const b64 = await page.evaluate(
      async ([uri, px]) => {
        const img = new Image();
        img.src = uri;
        await img.decode();
        const canvas = document.createElement("canvas");
        canvas.width = px;
        canvas.height = px;
        canvas.getContext("2d").drawImage(img, 0, 0, px, px);
        return canvas.toDataURL("image/png").split(",")[1];
      },
      [svgUri(svg), size],
    );
    return Buffer.from(b64, "base64");
  };

  // Application : PNG et ICO.
  const frames = [];
  for (const { size, svg } of ICO_FRAMES) frames.push({ size, png: await png(svg, size) });
  write(join(icons, "icon.ico"), buildIco(frames));
  write(join(icons, "32x32.png"), frames.find((f) => f.size === 32).png);
  write(join(icons, "64x64.png"), frames.find((f) => f.size === 64).png);
  write(join(icons, "128x128.png"), await png("app-icon.svg", 128));
  write(join(icons, "128x128@2x.png"), frames.find((f) => f.size === 256).png);
  write(join(icons, "icon.png"), await png("app-icon.svg", 512));

  // Zone de notification : une image par état et par taille.
  rmSync(join(icons, "tray.png"), { force: true });
  for (const state of TRAY_STATES) {
    for (const size of TRAY_SIZES) {
      write(
        join(icons, "tray", `tray-${state}-${size}.png`),
        await png(`tray/tray-${state}.svg`, size),
      );
    }
  }

  // Bandeau de l'installateur NSIS : 164 × 314, fond de l'app, la braise au centre, le logo au-dessus.
  const sidebar = await page.evaluate(
    async ([uri]) => {
      const img = new Image();
      img.src = uri;
      await img.decode();
      const [w, h] = [164, 314];
      const canvas = document.createElement("canvas");
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext("2d");
      ctx.fillStyle = "#1c1518";
      ctx.fillRect(0, 0, w, h);
      const glow = ctx.createRadialGradient(w / 2, 120, 4, w / 2, 120, 120);
      glow.addColorStop(0, "rgba(255,123,61,0.30)");
      glow.addColorStop(1, "rgba(255,123,61,0)");
      ctx.fillStyle = glow;
      ctx.fillRect(0, 0, w, h);
      ctx.drawImage(img, (w - 96) / 2, 72, 96, 96);
      ctx.fillStyle = "#2a2024";
      ctx.fillRect(0, h - 6, w, 6);
      ctx.fillStyle = "#ff7b3d";
      ctx.fillRect(0, h - 6, 48, 6);
      return Array.from(ctx.getImageData(0, 0, w, h).data);
    },
    [svgUri("logo.svg")],
  );
  write(join(installer, "sidebar.bmp"), buildBmp(164, 314, sidebar));

  await browser.close();
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
